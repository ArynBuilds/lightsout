//! Lights Out terminal: the same app on Windows and in Lights Out.
//! Tabs with split panes, agent status read from the screen, alerts, a
//! control API for agents, sessions that survive restarts and switching.
//! Everything is reachable from one key: Ctrl+Shift+P.

mod api;
mod commands;
mod config;
mod headless;
mod keys;
mod layout;
mod parts;
mod pictures;
mod platform;
mod plugins;
mod render;
mod session;
mod status;
mod tab;
mod tui;

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use commands::Action;
use layout::{Node, Rect};
use platform::Lock;
use render::{Canvas, Fonts};
use serde_json::{json, Value};
use session::{Saved, SavedPane, SavedTab};
use status::State;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tab::{Pane, Size, UserEvent, Waker};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

const PALETTE: [u32; 16] = [
    0x1c1c1c, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd6d6d6, 0x5c6370,
    0xff7b86, 0xb5e890, 0xffd88a, 0x82c4ff, 0xdd9bf0, 0x7fd6e0, 0xffffff,
];
const BAR: u32 = 0x000000; // full black: OLED pixels stay off
const BAR_ACTIVE: u32 = 0x000000;
const DIMTEXT: u32 = 0x7a7a85;
const ALERT: u32 = 0xff6b5b;
/// Lights Out on a PC: at this battery level a warning (and "plug in!" next to it)...
const LOW_BATTERY: u8 = 10;
/// ...and at this one the tabs are saved and the PC shuts down cleanly.
const EMPTY_BATTERY: u8 = 2;
const DONE: u32 = 0x6fcf7f;
const BORDER: u32 = 0x26262c;
const PADX: i32 = 6;
/// At most ~30 redraws per second, only when something changed.
const FRAME: Duration = Duration::from_millis(33);
// On battery, while you aren't typing or pointing: ~4 a second. Every screen update wakes the
// display (the panel stops refreshing itself), so agent output comes in fewer, bigger steps.
const FRAME_ON_BATTERY: Duration = Duration::from_millis(250);
/// After your last key or mouse use, this long at full speed (what you do shows at once).
const ACTIVE: Duration = Duration::from_secs(1);

fn rgb(c: Rgb) -> u32 {
    (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
}
fn dim(c: u32) -> u32 {
    let f = |s: u32| (((c >> s) & 0xff) * 2 / 3) << s;
    f(16) | f(8) | f(0)
}
fn xterm256(i: u8) -> u32 {
    match i {
        0..=15 => PALETTE[i as usize],
        16..=231 => {
            let i = i - 16;
            let v = |x: u8| if x == 0 { 0 } else { 55 + x as u32 * 40 };
            v(i / 36) << 16 | v((i / 6) % 6) << 8 | v(i % 6)
        }
        _ => {
            let g = 8 + (i as u32 - 232) * 10;
            g << 16 | g << 8 | g
        }
    }
}

#[derive(Clone, Copy)]
struct Theme {
    bg: u32,
    fg: u32,
    accent: u32,
}

fn resolve(c: Color, colors: &alacritty_terminal::term::color::Colors, t: Theme) -> u32 {
    match c {
        Color::Spec(r) => rgb(r),
        Color::Indexed(i) => colors[i as usize].map(rgb).unwrap_or_else(|| xterm256(i)),
        Color::Named(n) => {
            let idx = n as usize;
            if let Some(r) = colors[idx] {
                return rgb(r);
            }
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => t.fg,
                NamedColor::Background => t.bg,
                NamedColor::DimForeground => dim(t.fg),
                _ if idx < 16 => PALETTE[idx],
                _ if (259..=266).contains(&idx) => dim(PALETTE[idx - 259]),
                _ => t.fg,
            }
        }
    }
}

/// A running program in the sidebar: pane id, name, state, folder.
type AgentRow = (u64, String, State, String);

/// ↑ and ↓ next to a list's heading: more above, more below.
fn scroll_marks(cv: &mut Canvas, f: &mut Fonts, x: i32, y: i32, up: bool, down: bool) {
    if up {
        cv.ch(f, x, y, '↑', false, 0x8a8a93);
    }
    if down {
        cv.ch(f, x + f.cell_w as i32, y, '↓', false, 0x8a8a93);
    }
}

/// Black and white only: the shape says the state.
/// ● waiting for you · ◌ working · ○ done · dim dot idle
fn state_mark(s: State) -> (char, u32) {
    match s {
        State::NeedsYou => ('●', 0xffffff),
        State::Working => ('◌', 0xd0d0d6),
        State::Done => ('○', 0xffffff),
        State::Idle => ('·', 0x55555c),
    }
}

fn urgency(s: State) -> u8 {
    match s {
        State::NeedsYou => 3,
        State::Done => 2,
        State::Working => 1,
        State::Idle => 0,
    }
}

// ---------------------------------------------------------------- app state

/// Where the parts of the screen are (see `screen_layout`).
#[derive(Default, Clone)]
struct ScreenLayout {
    sidebar: Option<Rect>,
    sidebar_right: bool,
    bar: Option<Rect>,
    bar_bottom: bool,
    /// Plugins' parts: their name (PLUGIN.PART), where, and their side.
    parts: Vec<(String, Rect, String)>,
    content: Rect,
}

/// A tab: panes in a tree of splits, and panes floating over it (last = on top).
struct TabView {
    layout: Node,
    focus: u64,
    floats: Vec<layout::Float>,
}

impl TabView {
    fn new(id: u64) -> Self {
        TabView {
            layout: Node::Leaf(id),
            focus: id,
            floats: Vec::new(),
        }
    }

    /// Every pane in the tab: the tree's, then the floating ones.
    fn panes(&self) -> Vec<u64> {
        let mut v = self.layout.leaves();
        v.extend(self.floats.iter().map(|f| f.id));
        v
    }

    fn contains(&self, id: u64) -> bool {
        self.panes().contains(&id)
    }

    fn float(&mut self, id: u64) -> Option<&mut layout::Float> {
        self.floats.iter_mut().find(|f| f.id == id)
    }

    /// Takes a pane out (the others keep their places); true when the tab is now empty. The
    /// focus moves to the top floating pane that shows, else the tree's first.
    fn remove(&mut self, id: u64) -> bool {
        self.floats.retain(|f| f.id != id);
        self.layout.remove(id);
        self.focus_away_from(id);
        self.panes().is_empty()
    }

    /// The focus leaves pane `id` (gone or minimised): to the top floating pane that shows,
    /// else the tree's first, else any (all minimised).
    fn focus_away_from(&mut self, id: u64) {
        if self.focus != id {
            return;
        }
        let shown = self
            .floats
            .iter()
            .rev()
            .find(|f| !f.min && f.id != id)
            .map(|f| f.id);
        let any = self.floats.iter().rev().find(|f| f.id != id).map(|f| f.id);
        if let Some(f) = shown
            .or_else(|| self.layout.leaves().first().copied())
            .or(any)
        {
            self.focus = f;
        }
    }

    /// A pane floats and is minimised (out of sight until it is focused or raised).
    fn minimize(&mut self, id: u64) {
        self.float_pane(id);
        if let Some(f) = self.float(id) {
            f.min = true;
        }
        self.focus_away_from(id);
    }

    /// A pane floats: out of the tree, over the others (a little lower and to the right of
    /// the one before, so none hides another).
    fn float_pane(&mut self, id: u64) {
        if !self.contains(id) || self.floats.iter().any(|f| f.id == id) {
            return;
        }
        self.layout.remove(id);
        let n = self.floats.len() as f32;
        self.floats.push(layout::Float {
            id,
            x: (0.12 + 0.04 * n) % 0.4,
            y: (0.08 + 0.05 * n) % 0.4,
            w: 0.6,
            h: 0.6,
            min: false,
        });
    }

    /// A floating pane goes back into the tree (next to the last one there).
    fn tile_pane(&mut self, id: u64) {
        if self.floats.iter().any(|f| f.id == id) {
            self.floats.retain(|f| f.id != id);
            self.layout.add(id);
        }
    }

    /// A floating pane comes up on top (and out of the minimised ones).
    fn raise(&mut self, id: u64) {
        if let Some(i) = self.floats.iter().position(|f| f.id == id) {
            let mut f = self.floats.remove(i);
            f.min = false;
            self.floats.push(f);
        }
    }
}

/// A room has its own set of tabs. The current room's tabs live in App.tabs;
/// the others are kept here.
#[derive(Default)]
struct Room {
    name: String,
    tabs: Vec<TabView>,
    active: usize,
    /// Lives on another machine: this room shows it, its agents run there.
    remote: Option<session::RemoteRoom>,
    /// What runs in it over there (name, state, folder), as that machine reports it.
    remote_agents: Vec<(String, String, String)>,
    /// Its server can't be reached right now.
    offline: bool,
}

#[derive(Clone, Copy)]
enum Purpose {
    Command,
    RoomName(usize),
    WifiPassword(usize),
    WifiName,
    ServerRoom,
}

struct Item {
    label: String,
    hint: String,
    action: Action,
}

enum Overlay {
    None,
    Palette {
        title: String,
        items: Vec<Item>,
        filter: String,
        sel: usize,
        /// ↑/↓ was used: `sel` is highlighted (and Enter runs it).
        picked: bool,
        /// The first row shown (the wheel scrolls it).
        top: usize,
    },
    Menu {
        x: i32,
        y: i32,
        items: Vec<Item>,
        sel: usize,
    },
    Confirm(Confirm),
    Ask {
        title: String,
        text: String,
        purpose: Purpose,
    },
}

/// A list that scrolls with the mouse wheel.
#[derive(Clone, Copy, PartialEq)]
enum Scroll {
    Rooms,
    Running,
    Tabs,
}

#[derive(Clone, Copy)]
enum Confirm {
    BackToWindows,
    EnterLightsOut,
    Quit,
}

struct Toast {
    text: String,
    color: u32,
    at: Instant,
}

struct Waiter {
    pane: u64,
    until: Vec<State>,
    deadline: Instant,
    reply: mpsc::Sender<Value>,
    since: Instant,
    /// "idle" only counts once the pane has done something (or after a grace period),
    /// so a program that is still starting up isn't mistaken for finished.
    seen_busy: bool,
}

enum Drag {
    Divider {
        tab: usize,
        path: Vec<bool>,
        side_by_side: bool,
        area: Rect,
    },
    Select {
        pane: u64,
        origin: Rect,
    },
    /// A floating pane's title bar is dragged (where the mouse took it, from its corner).
    Float {
        pane: u64,
        dx: i32,
        dy: i32,
    },
    /// A floating pane's corner is dragged.
    FloatSize {
        pane: u64,
    },
}

#[derive(Clone, Copy)]
enum Hit {
    /// A row of the actions list (its place among the shown rows).
    PaletteItem(usize),
    /// The actions box itself (a click there doesn't close it).
    PaletteBox,
    TabsLeft,
    TabsRight,
    Tab(usize),
    NewTab,
    Palette,
    Switch,
    Pane(u64),
    Room(usize),
    NewRoom,
    /// A piece of a plugin's part that runs a command (its place in `part_clicks`).
    Part(usize),
    /// A floating pane's title bar (drag to move), its buttons, its resize corner.
    FloatTitle(u64),
    FloatMin(u64),
    FloatTile(u64),
    FloatClose(u64),
    FloatCorner(u64),
}

struct App {
    proxy: Waker,
    /// Set to end the app (the window's loop or the terminal's loop ends then).
    pub quit: bool,
    /// A redraw is due (the loop that owns the screen picks it up).
    redraw: bool,
    /// Running inside another terminal (`lightsout` typed in a shell): cells, not pixels.
    pub tui: bool,
    /// Shown inside a room on another machine: just this room (no sidebar, no own buttons).
    pub embedded: bool,
    /// The terminal view asked to detach (the background copy keeps running).
    detach: bool,
    session: String,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    _context: Option<softbuffer::Context<Rc<Window>>>,
    fonts: Fonts,
    theme: Theme,
    cfg: config::Config,
    cfg_modified: Option<std::time::SystemTime>,
    cfg_checked: Instant,
    panes: HashMap<u64, Pane>,
    tabs: Vec<TabView>,
    active: usize,
    popup: Option<u64>,
    next_id: u64,
    mods: ModifiersState,
    size: (u32, u32),
    dirty: bool,
    last_draw: Instant,
    overlay: Overlay,
    toasts: Vec<Toast>,
    screen_off: bool,
    lock: Lock,
    pin: String,
    mouse: PhysicalPosition<f64>,
    drag: Option<Drag>,
    hits: Vec<(Rect, Hit)>,
    /// A mouse button held in a pane whose program gets the mouse (pane, button).
    mouse_held: Option<(u64, u8)>,
    /// How many rows the actions list showed last time (to keep the chosen one in view).
    palette_rows: usize,
    /// Lists that scroll with the mouse wheel: where they are on screen, and their first item.
    scroll_areas: Vec<(Rect, Scroll)>,
    /// The part of a line the wheel or touchpad moved that isn't a whole line yet.
    wheel_rest: f64,
    rooms_top: usize,
    running_top: usize,
    tabs_first: usize,
    /// The room and tab last shown, to bring a newly chosen one into view once.
    seen_room: usize,
    seen_tab: usize,
    battery_text: String,
    /// On battery: fewer redraws per second.
    on_battery: bool,
    /// Your last key or mouse use.
    last_input: Instant,
    /// The frame on screen, so the compositor is told only what changed.
    shown: Vec<u32>,
    battery_at: Instant,
    /// The low-battery warning was shown (once until the PC charges).
    battery_warned: bool,
    /// The time left, averaged over readings (one busy moment doesn't swing it).
    hours_avg: Option<f32>,
    bar_texts: Vec<String>,
    clipboard: Option<arboard::Clipboard>,
    switching: Option<mpsc::Receiver<Result<(), String>>>,
    /// What runs in each tab's shell (shell pid -> command line), fetched off the window's
    /// thread whenever it changes, so saving tabs never waits for it.
    commands: HashMap<u32, Option<String>>,
    commands_stale: bool,
    commands_rx: Option<mpsc::Receiver<HashMap<u32, Option<String>>>>,
    wifi_nets: Vec<platform::Net>,
    wifi_scan: Option<mpsc::Receiver<Vec<platform::Net>>>,
    wifi_join: Option<mpsc::Receiver<Result<String, String>>>,
    waiters: Vec<Waiter>,
    status_at: Instant,
    saved_at: Instant,
    /// The rooms, tabs, folders and programs as last saved (without the screens), and when
    /// that was last compared: any change to them is saved within a couple of seconds.
    saved_shape: String,
    /// Quit or a switch has begun (see `begin_closing`).
    closing: bool,
    shape_at: Instant,
    shape_check: bool,
    window_focused: bool,
    hidden: bool,
    sidebar: bool,
    running_at: Instant,
    rooms: Vec<Room>,
    room: usize,
    /// The plugins that are on (plugins.rs), and how their files looked when read.
    plugins: Vec<plugins::Plugin>,
    plugins_stamp: Vec<Option<std::time::SystemTime>>,
    /// Plugin keys already said to do nothing (each said once).
    keys_warned: Vec<String>,
    /// What each plugin part shows (PLUGIN.PART), as its program last sent it: rows, or a
    /// picture it drew (a frame).
    part_content: HashMap<String, parts::Content>,
    part_images: HashMap<String, parts::Image>,
    /// How far down a picture taller than its part is scrolled (pixels from its top).
    part_scroll: HashMap<String, u32>,
    /// Each part's size as its plugin was last told (cols, rows, width, height).
    part_sizes: HashMap<String, (i32, i32, i32, i32)>,
    /// The commands of the pieces of parts on screen (a click finds them by `Hit::Part`).
    part_clicks: Vec<String>,
    /// Parts whose program has been started (once each).
    parts_started: Vec<String>,
    /// `start` has run: the control port answers, so part programs can be started.
    started: bool,
    /// Plugins off (`lightsout --plain`, or Ctrl+Shift+F12 until pressed again): the way
    /// back when a plugin broke the screen.
    plain: bool,
    /// Inside a terminal that draws pictures: which way (kitty, iterm, sixel) and its cell
    /// size in pixels (plugins' frames show there, at that size).
    pub tui_gfx: Option<(String, (i32, i32))>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UiState {
    sidebar: bool,
}

impl Default for UiState {
    fn default() -> Self {
        UiState { sidebar: true } // rooms and agents live in the sidebar
    }
}

fn load_ui() -> UiState {
    std::fs::read_to_string(api::state_dir().join("ui.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_ui(u: &UiState) {
    if let Ok(s) = serde_json::to_string(u) {
        let _ = std::fs::write(api::state_dir().join("ui.json"), s);
    }
}

fn theme_from(c: &config::Config) -> Theme {
    Theme {
        bg: config::color(&c.theme.background, 0x000000),
        fg: config::color(&c.theme.foreground, 0xd6d6d6),
        accent: config::color(&c.theme.accent, 0xf5c542),
    }
}

impl App {
    pub(crate) fn new(proxy: Waker, session: String) -> Self {
        let loaded = config::load();
        let theme = theme_from(&loaded.config);
        let mut app = App {
            proxy,
            quit: false,
            redraw: false,
            tui: false,
            embedded: false,
            detach: false,
            session,
            window: None,
            surface: None,
            _context: None,
            fonts: Fonts::new(15.0),
            theme,
            cfg: loaded.config,
            cfg_modified: loaded.modified,
            cfg_checked: Instant::now(),
            panes: HashMap::new(),
            tabs: Vec::new(),
            active: 0,
            popup: None,
            next_id: 1,
            mods: ModifiersState::empty(),
            size: (800, 600),
            dirty: true,
            last_draw: Instant::now() - FRAME,
            overlay: Overlay::None,
            toasts: Vec::new(),
            screen_off: false,
            lock: platform::initial_lock(),
            pin: String::new(),
            mouse: PhysicalPosition::new(0.0, 0.0),
            drag: None,
            hits: Vec::new(),
            mouse_held: None,
            palette_rows: 16,
            scroll_areas: Vec::new(),
            wheel_rest: 0.0,
            rooms_top: 0,
            running_top: 0,
            tabs_first: 0,
            seen_room: usize::MAX,
            seen_tab: usize::MAX,
            battery_text: String::new(),
            on_battery: false,
            last_input: Instant::now(),
            shown: Vec::new(),
            battery_at: Instant::now() - Duration::from_secs(60),
            battery_warned: false,
            hours_avg: None,
            bar_texts: Vec::new(),
            clipboard: arboard::Clipboard::new().ok(),
            switching: None,
            commands: HashMap::new(),
            commands_stale: true,
            commands_rx: None,
            wifi_nets: Vec::new(),
            wifi_scan: None,
            wifi_join: None,
            waiters: Vec::new(),
            status_at: Instant::now(),
            saved_at: Instant::now(),
            saved_shape: String::new(),
            closing: false,
            shape_at: Instant::now(),
            shape_check: false,
            window_focused: true,
            hidden: false,
            sidebar: load_ui().sidebar,
            running_at: Instant::now() - Duration::from_secs(10),
            rooms: vec![Room {
                name: "Room 1".into(),
                tabs: Vec::new(),
                active: 0,
                ..Default::default()
            }],
            room: 0,
            plugins: Vec::new(),
            plugins_stamp: Vec::new(),
            keys_warned: Vec::new(),
            part_content: HashMap::new(),
            part_images: HashMap::new(),
            part_scroll: HashMap::new(),
            part_sizes: HashMap::new(),
            part_clicks: Vec::new(),
            parts_started: Vec::new(),
            started: false,
            plain: std::env::var("LIGHTSOUT_PLAIN").is_ok_and(|v| v == "1"),
            tui_gfx: None,
        };
        if let Some(e) = loaded.error {
            app.toast(e, ALERT);
        }
        app.load_plugins();
        app
    }

    // ------------------------------------------------------------ geometry

    fn bar_h(&self) -> i32 {
        if self.tui {
            return 1;
        }
        self.fonts.cell_h as i32 + 10
    }

    /// Space between a pane's edge and its text: pixels in the window, none in a terminal.
    fn padx(&self) -> i32 {
        if self.tui {
            0
        } else {
            PADX
        }
    }

    fn pady(&self) -> i32 {
        if self.tui {
            0
        } else {
            2
        }
    }

    /// The first row of a menu, below its top edge.
    fn menu_inset(&self) -> i32 {
        if self.tui {
            1
        } else {
            4
        }
    }

    /// Where the panes are: what the parts around them leave.
    fn content(&self) -> Rect {
        self.screen_layout().content
    }

    /// Where everything is: the sidebar (left, right or off), plugins' parts on the left and
    /// right, all full height; then the tab bar (top, bottom or off) and plugins' parts at the
    /// top and bottom, across what is left; the panes in the rest. A plugin part that replaces
    /// the sidebar or the tab bar takes its place. A part is made smaller when the panes would
    /// get too little room, and left out when it would be tiny.
    fn screen_layout(&self) -> ScreenLayout {
        let mut area = Rect {
            x: 0,
            y: 0,
            w: self.size.0 as i32,
            h: self.size.1 as i32,
        };
        let (cw, ch) = if self.tui {
            (1, 1)
        } else {
            (self.fonts.cell_w as i32, self.fonts.cell_h as i32)
        };
        let (min_w, min_h) = (20 * cw, 5 * ch);
        let plugin_parts: Vec<(String, plugins::PluginPart)> = if self.embedded {
            Vec::new()
        } else {
            self.room_plugins()
                .flat_map(|p| {
                    p.m.parts
                        .iter()
                        .filter(|x| plugins::runs_here(&x.platforms))
                        .map(|x| (format!("{}.{}", p.m.id, x.id), x.clone()))
                        // one shown only when filled takes no room while it is empty
                        .filter(|(id, x)| {
                            x.show != "when-filled"
                                || self.part_content.contains_key(id)
                                || self.part_images.contains_key(id)
                        })
                })
                .collect()
        };
        let replaced = |what: &str| plugin_parts.iter().any(|(_, x)| x.replaces == what);
        let mut out = ScreenLayout::default();
        // left and right, full height
        let side = if replaced("sidebar") {
            "off"
        } else {
            self.cfg.parts.sidebar.as_str()
        };
        let sw = self.sidebar_w();
        if sw > 0 && side != "off" && area.w - sw >= min_w {
            let right = side == "right";
            out.sidebar = Some(Rect {
                x: if right { area.x + area.w - sw } else { area.x },
                w: sw,
                ..area
            });
            out.sidebar_right = right;
            if !right {
                area.x += sw;
            }
            area.w -= sw;
        }
        for (id, p) in plugin_parts
            .iter()
            .filter(|(_, p)| p.side == "left" || p.side == "right")
        {
            // as wide as asked, or what the panes can spare (left out only if that is tiny)
            let want = p.size as i32 * cw + if self.tui { 0 } else { 2 * PADX };
            let pw = want.min(area.w - min_w);
            if pw < 4 * cw {
                continue;
            }
            let right = p.side == "right";
            out.parts.push((
                id.clone(),
                Rect {
                    x: if right { area.x + area.w - pw } else { area.x },
                    w: pw,
                    ..area
                },
                p.side.clone(),
            ));
            if !right {
                area.x += pw;
            }
            area.w -= pw;
        }
        // top and bottom, across what is left
        let tabs = if replaced("tabs") {
            "off"
        } else {
            self.cfg.parts.tabs.as_str()
        };
        let bh = self.bar_h();
        if tabs != "off" && area.h - bh >= min_h {
            let bottom = tabs == "bottom";
            out.bar = Some(Rect {
                y: if bottom { area.y + area.h - bh } else { area.y },
                h: bh,
                ..area
            });
            out.bar_bottom = bottom;
            if !bottom {
                area.y += bh;
            }
            area.h -= bh;
        }
        for (id, p) in plugin_parts
            .iter()
            .filter(|(_, p)| p.side == "top" || p.side == "bottom")
        {
            let want = p.size as i32 * ch + if self.tui { 0 } else { 8 };
            let ph = want.min(area.h - min_h);
            if ph < ch {
                continue;
            }
            let bottom = p.side == "bottom";
            out.parts.push((
                id.clone(),
                Rect {
                    y: if bottom { area.y + area.h - ph } else { area.y },
                    h: ph,
                    ..area
                },
                p.side.clone(),
            ));
            if !bottom {
                area.y += ph;
            }
            area.h -= ph;
        }
        out.content = area;
        out
    }

    /// Your colours, for plugins (LIGHTSOUT_THEME): from the settings, so plugins from
    /// different people match.
    fn theme_json(&self) -> Value {
        let hex = |c: u32| format!("#{c:06x}");
        json!({
            "background": hex(self.theme.bg),
            "foreground": hex(self.theme.fg),
            "accent": hex(self.theme.accent),
            "dim": hex(DIMTEXT),
            "alert": hex(ALERT),
            "done": hex(DONE),
            "font_size": self.cfg.theme.font_size,
        })
    }

    /// The plugins that are on in the room you're in (`lightsout plugin on ID --room NAME`).
    fn room_plugins(&self) -> impl Iterator<Item = &plugins::Plugin> + '_ {
        let room = self
            .rooms
            .get(self.room)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        self.plugins.iter().filter(move |p| p.in_room(&room))
    }

    /// The colours a plugin's part can name (from the settings).
    fn palette(&self) -> parts::Palette {
        parts::Palette {
            fg: self.theme.fg,
            bg: self.theme.bg,
            accent: self.theme.accent,
            dim: DIMTEXT,
            alert: ALERT,
            done: DONE,
        }
    }

    /// The agent list on the left (Ctrl+Shift+B), about 26 characters wide.
    fn sidebar_w(&self) -> i32 {
        if self.tui {
            return if self.sidebar && !self.embedded && self.size.0 > 70 {
                24
            } else {
                0
            };
        }
        if self.sidebar && self.size.0 as i32 > 60 * self.fonts.cell_w as i32 {
            26 * self.fonts.cell_w as i32
        } else {
            0
        }
    }

    fn popup_rect(&self) -> Rect {
        let c = self.content();
        let (w, h) = (c.w * 8 / 10, c.h * 8 / 10);
        Rect {
            x: c.x + (c.w - w) / 2,
            y: c.y + (c.h - h) / 2,
            w,
            h,
        }
    }

    fn grid_size(&self, r: Rect) -> Size {
        if self.tui {
            return Size {
                cols: r.w.max(2) as usize,
                rows: r.h.max(2) as usize,
            };
        }
        let cols = ((r.w - 2 * PADX).max(0) as u32 / self.fonts.cell_w).max(2) as usize;
        let rows = ((r.h - 4).max(0) as u32 / self.fonts.cell_h).max(2) as usize;
        Size { cols, rows }
    }

    fn cell(&self) -> (u32, u32) {
        (self.fonts.cell_w, self.fonts.cell_h)
    }

    /// A floating pane's title bar: one row of cells inside a terminal, a little taller in the
    /// window.
    fn title_h(&self) -> i32 {
        if self.tui {
            1
        } else {
            self.fonts.cell_h as i32 + 6
        }
    }

    /// Where a floating pane is on screen, title bar included (never smaller than a short
    /// line of text and a few rows).
    fn float_frame(&self, f: &layout::Float) -> Rect {
        let (cw, ch) = if self.tui {
            (1, 1)
        } else {
            (self.fonts.cell_w as i32, self.fonts.cell_h as i32)
        };
        f.frame(self.content(), 24 * cw, self.title_h() + 4 * ch)
    }

    /// The part of a floating pane's frame its program shows in (below the title bar; in the
    /// window, inside a thin border).
    fn float_body(&self, frame: Rect) -> Rect {
        let (b, t) = if self.tui {
            (0, 1)
        } else {
            (1, self.title_h())
        };
        Rect {
            x: frame.x + b,
            y: frame.y + t,
            w: frame.w - 2 * b,
            h: frame.h - t - b,
        }
    }

    /// The floating panes of the tab you see that aren't minimised, bottom to top, with their
    /// frames.
    fn float_frames(&self) -> Vec<(u64, Rect)> {
        self.tabs
            .get(self.active)
            .map(|t| {
                t.floats
                    .iter()
                    .filter(|f| !f.min)
                    .map(|f| (f.id, self.float_frame(f)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every pane you see and where its program shows: the tree's, then the floating ones
    /// from bottom to top (so the last one that contains a point is the one on top).
    fn active_rects(&self) -> Vec<(u64, Rect)> {
        let mut v = self
            .tabs
            .get(self.active)
            .map(|t| t.layout.layout(self.content()).0)
            .unwrap_or_default();
        for (id, frame) in self.float_frames() {
            v.push((id, self.float_body(frame)));
        }
        v
    }

    fn resize_all(&mut self) {
        let content = self.content();
        let cell = self.cell();
        let mut sizes = Vec::new();
        for t in self
            .tabs
            .iter()
            .chain(self.rooms.iter().flat_map(|r| r.tabs.iter()))
        {
            for (id, r) in t.layout.layout(content).0 {
                sizes.push((id, self.grid_size(r)));
            }
            // minimised ones keep their size for when they come back
            for f in &t.floats {
                let body = self.float_body(self.float_frame(f));
                sizes.push((f.id, self.grid_size(body)));
            }
        }
        if let Some(p) = self.popup {
            sizes.push((p, self.grid_size(self.popup_rect())));
        }
        for (id, s) in sizes {
            if let Some(p) = self.panes.get_mut(&id) {
                p.resize(s, cell);
            }
        }
        self.dirty = true;
        self.tell_part_sizes();
    }

    /// A part's size in pixels: in the window, as it is; inside a terminal that draws
    /// pictures, its cells times the terminal's cell size; inside any other, in cells (no
    /// picture shows there).
    fn part_pixels(&self, r: Rect) -> (i32, i32) {
        match (&self.tui_gfx, self.tui) {
            (Some((_, (cw, ch))), true) => (r.w * cw, r.h * ch),
            _ => (r.w, r.h),
        }
    }

    /// A plugin part got a new size (the window, the font, other parts changed): its plugin
    /// hears it (`part.resized`), so a picture can be drawn again at the new size.
    fn tell_part_sizes(&mut self) {
        if !self.started {
            return;
        }
        let (cw, ch) = if self.tui {
            (1, 1)
        } else {
            (
                self.fonts.cell_w.max(1) as i32,
                self.fonts.cell_h.max(1) as i32,
            )
        };
        let shown = self.screen_layout().parts;
        // one that went (shown only when filled) is told its size again when it comes back
        self.part_sizes
            .retain(|id, _| shown.iter().any(|(x, _, _)| x == id));
        let changed: Vec<(String, (i32, i32, i32, i32))> = shown
            .into_iter()
            .map(|(id, r, _)| {
                let (w, h) = self.part_pixels(r);
                (id, (r.w / cw, r.h / ch, w, h))
            })
            .filter(|(id, size)| self.part_sizes.get(id) != Some(size))
            .collect();
        for (id, size) in changed {
            self.part_sizes.insert(id.clone(), size);
            let (cols, rows, width, height) = size;
            self.emit(
                "part.resized",
                json!({ "part": id, "cols": cols, "rows": rows, "width": width, "height": height }),
            );
        }
    }

    // ------------------------------------------------------------ panes and tabs

    fn spawn(
        &mut self,
        cwd: PathBuf,
        command: Option<String>,
        env: Vec<(String, String)>,
        size: Size,
    ) -> Option<u64> {
        let cwd = if cwd.is_dir() { cwd } else { platform::home() };
        if platform::in_lightsout() {
            let _ = std::process::Command::new("sudo")
                .args(["-n", "/usr/local/sbin/lightsout-split"])
                .arg(&cwd)
                .status();
        }
        let (program, args) = match &command {
            Some(c) => platform::agent(c),
            None => platform::shell(),
        };
        let id = self.next_id;
        self.next_id += 1;
        match Pane::spawn(
            id,
            &self.proxy,
            &self.session,
            cwd,
            program,
            args,
            command,
            size,
            self.cell(),
            env,
        ) {
            Ok(p) => {
                let data =
                    json!({ "pane": id, "command": p.command, "cwd": p.cwd.to_string_lossy() });
                self.panes.insert(id, p);
                self.dirty = true;
                self.emit("pane.opened", data);
                Some(id)
            }
            Err(e) => {
                self.toast(format!("Could not start: {e}"), ALERT);
                None
            }
        }
    }

    fn new_tab(
        &mut self,
        cwd: PathBuf,
        command: Option<String>,
        env: Vec<(String, String)>,
    ) -> Option<u64> {
        let size = self.grid_size(self.content());
        let id = self.spawn(cwd, command, env, size)?;
        self.tabs.push(TabView::new(id));
        self.active = self.tabs.len() - 1;
        Some(id)
    }

    fn split(
        &mut self,
        side_by_side: bool,
        command: Option<String>,
        target: Option<u64>,
        env: Vec<(String, String)>,
    ) -> Option<u64> {
        let target = target.or_else(|| self.tabs.get(self.active).map(|t| t.focus))?;
        let ti = self.tab_of(target)?;
        let cwd = self
            .panes
            .get(&target)
            .map(|p| p.cwd.clone())
            .unwrap_or_else(platform::home);
        let size = self.grid_size(self.content());
        let id = self.spawn(cwd, command, env, size)?;
        let tab = &mut self.tabs[ti];
        if !tab.layout.split(target, id, side_by_side) {
            // a floating pane split: the new one floats too, next to it
            let next = tab.floats.iter().find(|f| f.id == target).map(|f| {
                let (dx, dy) = if side_by_side {
                    (0.05, 0.0)
                } else {
                    (0.0, 0.05)
                };
                layout::Float {
                    id,
                    x: (f.x + dx).min(0.9),
                    y: (f.y + dy).min(0.9),
                    min: false,
                    ..f.clone()
                }
            });
            match next {
                Some(f) => tab.floats.push(f),
                None => tab.layout.add(id),
            }
        }
        tab.focus = id;
        self.active = ti;
        self.resize_all();
        Some(id)
    }

    fn tab_of(&self, pane: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.contains(pane))
    }

    /// The pane a pane command is for (the one named, else the one you're in; a popup is
    /// never one) and its tab, in the room you see (a pane in another room brings you there).
    fn target_pane(&mut self, pane: Option<u64>) -> Option<(usize, u64)> {
        let id = pane.or_else(|| self.tabs.get(self.active).map(|t| t.focus))?;
        if let Some(r) = self.room_of(id) {
            self.switch_room(r);
        }
        Some((self.tab_of(id)?, id))
    }

    fn focused(&self) -> Option<u64> {
        self.popup
            .or_else(|| self.tabs.get(self.active).map(|t| t.focus))
    }

    fn cwd(&self) -> PathBuf {
        self.focused()
            .and_then(|id| self.panes.get(&id))
            .map(|p| p.cwd.clone())
            .unwrap_or_else(platform::home)
    }

    /// Which room a pane is in.
    fn room_of(&self, pane: u64) -> Option<usize> {
        if self.tab_of(pane).is_some() {
            return Some(self.room);
        }
        self.rooms
            .iter()
            .position(|r| r.tabs.iter().any(|t| t.contains(pane)))
    }

    fn switch_room(&mut self, i: usize) {
        if i == self.room || i >= self.rooms.len() {
            return;
        }
        let cur = self.room;
        self.rooms[cur].tabs = std::mem::take(&mut self.tabs);
        self.rooms[cur].active = self.active;
        self.tabs = std::mem::take(&mut self.rooms[i].tabs);
        self.active = self.rooms[i].active.min(self.tabs.len().saturating_sub(1));
        self.room = i;
        // a plugin on in some rooms only: its parts come or go with the room
        if self
            .plugins
            .iter()
            .any(|p| !p.only_rooms.is_empty() || !p.not_rooms.is_empty())
        {
            self.resize_all();
        }
        if let Some(f) = self.tabs.get(self.active).map(|t| t.focus) {
            if let Some(p) = self.panes.get_mut(&f) {
                p.det.seen();
            }
        }
        self.dirty = true;
    }

    /// A new room (named, or "Room N"), with a tab, and you in it.
    fn new_room(&mut self, name: Option<&str>) {
        let cwd = self.cwd();
        let name = name
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Room {}", self.rooms.len() + 1));
        self.rooms.push(Room {
            name: name.clone(),
            tabs: Vec::new(),
            active: 0,
            ..Default::default()
        });
        self.switch_room(self.rooms.len() - 1);
        self.new_tab(cwd, None, Vec::new());
        self.emit("room.opened", json!({ "room": name }));
    }

    /// Rooms on servers (server-rooms.json): they appear when added and go when removed.
    fn sync_server_rooms(&mut self) {
        let servers = session::server_rooms();
        let current = self.room;
        let mut keep_current = true;
        // removed: the room closes here (its agents keep running on the server)
        for i in (0..self.rooms.len()).rev() {
            let Some(remote) = self.rooms[i].remote.clone() else {
                continue;
            };
            match servers.iter().find(|m| m.name == remote.server) {
                Some(m) if m.ssh == remote.ssh => {}
                Some(m) => {
                    if let Some(r) = self.rooms[i].remote.as_mut() {
                        r.ssh = m.ssh.clone(); // reached another way from now on (next connect)
                    }
                }
                None => {
                    if i == self.room {
                        keep_current = false;
                    } else {
                        for id in self.rooms[i].tabs.iter().flat_map(|t| t.panes()) {
                            if let Some(p) = self.panes.remove(&id) {
                                p.shutdown();
                            }
                        }
                        self.rooms.remove(i);
                        if i < self.room {
                            self.room -= 1;
                        }
                    }
                }
            }
        }
        if !keep_current {
            self.close_room();
        }
        // added: a room each, connected (the room you're in stays)
        let back = if keep_current {
            Some(self.room.min(current))
        } else {
            None
        };
        let mut added = None;
        for m in &servers {
            if self
                .rooms
                .iter()
                .any(|r| r.remote.as_ref().is_some_and(|x| x.server == m.name))
            {
                continue;
            }
            let remote = session::RemoteRoom {
                server: m.name.clone(),
                ssh: m.ssh.clone(),
                room: String::new(),
            };
            self.rooms.push(Room {
                name: m.name.clone(),
                remote: Some(remote.clone()),
                ..Default::default()
            });
            let ri = self.rooms.len() - 1;
            self.switch_room(ri);
            self.open_remote(&remote);
            added = Some(ri);
        }
        if let (Some(b), Some(_)) = (back, added) {
            self.switch_room(b);
        }
        self.dirty = true;
    }

    /// Connects the current tab list to a room on another machine (and keeps reconnecting).
    fn open_remote(&mut self, remote: &session::RemoteRoom) {
        let mut exe = std::env::current_exe().unwrap_or_default();
        exe.set_file_name(if cfg!(windows) {
            "lightsout.exe"
        } else {
            "lightsout"
        });
        let exe = shell_word(&exe.to_string_lossy());
        let call = if cfg!(windows) {
            format!("& {exe}")
        } else {
            exe
        };
        let room: String = remote
            .room
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        // a server room shows what is open over there; a named room shows that room
        let which = if room.is_empty() {
            "--embed".to_string()
        } else {
            format!("--room {room}")
        };
        let cmd = format!("{call} open {} {which}", shell_word(&remote.ssh));
        let size = self.grid_size(self.content());
        if let Some(id) = self.spawn(platform::home(), Some(cmd), Vec::new(), size) {
            if let Some(p) = self.panes.get_mut(&id) {
                p.title = remote.server.clone();
            }
            self.tabs.push(TabView::new(id));
            self.active = self.tabs.len() - 1;
        }
    }

    /// Goes to the room with that name (made if there is none): a computer showing it as a server room asked.
    pub(crate) fn open_room_named(&mut self, name: &str) {
        if let Some(i) = self.rooms.iter().position(|r| r.name == name) {
            self.switch_room(i);
        } else {
            self.new_room(Some(name));
        }
    }

    /// The current room is empty: drop it and go to a neighbour (or start fresh).
    fn leave_empty_room(&mut self) {
        if self.rooms.len() > 1 {
            let gone = self.room;
            self.rooms.remove(gone);
            let next = gone.min(self.rooms.len() - 1);
            self.tabs = std::mem::take(&mut self.rooms[next].tabs);
            self.active = self.rooms[next]
                .active
                .min(self.tabs.len().saturating_sub(1));
            self.room = next;
        } else if platform::in_lightsout() {
            self.new_tab(platform::home(), None, Vec::new());
        } else {
            // the last tab closed: start fresh next time
            let _ = std::fs::remove_file(session::local_file(&self.session));
            self.quit = true;
        }
    }

    fn close_room(&mut self) {
        if let Some(name) = self.rooms.get(self.room).map(|r| r.name.clone()) {
            self.emit("room.closed", json!({ "room": name }));
        }
        // a server room: it leaves the list here (its agents keep running on the server)
        if let Some(m) = self
            .rooms
            .get(self.room)
            .and_then(|r| r.remote.as_ref())
            .map(|r| r.server.clone())
        {
            let mut all = session::server_rooms();
            all.retain(|x| x.name != m);
            let _ = session::save_server_rooms(&all);
        }
        let ids: Vec<u64> = self.tabs.iter().flat_map(|t| t.panes()).collect();
        for id in ids {
            if let Some(p) = self.panes.remove(&id) {
                p.shutdown();
            }
        }
        self.tabs.clear();
        self.leave_empty_room();
        self.resize_all();
    }

    fn focus(&mut self, pane: u64) {
        if let Some(r) = self.room_of(pane) {
            self.switch_room(r);
        }
        if let Some(ti) = self.tab_of(pane) {
            self.active = ti;
            self.tabs[ti].focus = pane;
            // a floating one comes up on top (and back if it was minimised)
            self.tabs[ti].raise(pane);
        }
        if let Some(p) = self.panes.get_mut(&pane) {
            p.det.seen();
        }
        self.dirty = true;
    }

    fn close_pane(&mut self, id: u64) {
        if self.closing {
            return;
        }
        if self.panes.contains_key(&id) {
            self.emit("pane.closed", json!({ "pane": id }));
        }
        // a pane in another room closes there, without switching rooms
        if self.popup != Some(id) {
            if let Some(r) = self.room_of(id).filter(|r| *r != self.room) {
                let tabs = &mut self.rooms[r].tabs;
                if let Some(ti) = tabs.iter().position(|t| t.contains(id)) {
                    if tabs[ti].remove(id) {
                        tabs.remove(ti);
                    }
                }
                if self.rooms[r].tabs.is_empty() {
                    self.rooms.remove(r);
                    if r < self.room {
                        self.room -= 1;
                    }
                }
                if let Some(p) = self.panes.remove(&id) {
                    p.shutdown();
                }
                self.resize_all();
                return;
            }
        }
        if self.popup == Some(id) {
            self.popup = None;
        } else if let Some(ti) = self.tab_of(id) {
            if self.tabs[ti].remove(id) {
                self.tabs.remove(ti);
                self.active = self.active.min(self.tabs.len().saturating_sub(1));
            }
        }
        if let Some(p) = self.panes.remove(&id) {
            p.shutdown();
        }
        if self.tabs.is_empty() {
            self.leave_empty_room();
        }
        self.resize_all();
    }

    fn toast(&mut self, text: String, color: u32) {
        self.toasts.push(Toast {
            text,
            color,
            at: Instant::now(),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
        self.dirty = true;
    }

    fn jump(&mut self) {
        let best = self
            .panes
            .values()
            .filter(|p| matches!(p.det.state, State::NeedsYou | State::Done))
            .max_by_key(|p| urgency(p.det.state))
            .map(|p| p.id);
        match best {
            Some(id) => self.focus(id),
            None => self.toast("No agent is waiting for you.".into(), DIMTEXT),
        }
    }

    // ------------------------------------------------------------ status and alerts

    fn status_tick(&mut self) {
        self.status_at = Instant::now();
        let focused = if self.window_focused && !self.hidden && !self.screen_off {
            self.focused()
        } else {
            None
        };
        let extra = self.cfg.status.needs_you.clone();
        let watching = !self.waiters.is_empty();
        let mut changes = Vec::new();
        for (id, p) in self.panes.iter_mut() {
            if !watching && !p.det.recently_active() && p.det.state == State::Idle {
                continue;
            }
            if p.body_changed() {
                p.det.on_work();
            }
            let tail = p.tail();
            if let Some(s) = p.det.update(&tail, focused == Some(*id), &extra) {
                changes.push((*id, s));
            }
        }
        if self.running_at.elapsed() >= Duration::from_millis(1500) {
            self.running_at = Instant::now();
            let pids: Vec<u32> = self.panes.values().filter_map(|p| p.shell_pid).collect();
            let running = platform::running_programs(&pids);
            platform::stay_awake(running.values().any(|r| r.is_some()));
            let mut events = Vec::new();
            for p in self.panes.values_mut() {
                let found = p
                    .shell_pid
                    .and_then(|pid| running.get(&pid).cloned().flatten());
                let pid = found.as_ref().map(|(pid, _)| *pid);
                // the program ended or another one runs: its conversation is gone with it (a
                // program just started keeps what its hook has already reported)
                if p.running_pid.is_some() && pid != p.running_pid {
                    p.agent_session = None;
                }
                p.running_pid = pid;
                // its record read again every time: the agent can open another conversation
                // (Claude's /resume), and a leftover record of an earlier process with the same
                // number is replaced once this one writes its own
                if let Some(id) = pid.and_then(platform::agent_session) {
                    p.agent_session = Some(id);
                }
                let now = found.map(|(_, name)| name);
                if now != p.running {
                    if let Some(old) = &p.running {
                        events.push(("program.ended", json!({ "pane": p.id, "program": old })));
                    }
                    if let Some(new) = &now {
                        events.push(("program.started", json!({ "pane": p.id, "program": new })));
                    }
                    p.running = now;
                    self.dirty = true;
                    self.commands_stale = true;
                }
            }
            for (e, data) in events {
                self.emit(e, data);
            }
        }
        for (id, s) in changes {
            self.dirty = true;
            let title = self.panes.get(&id).map(|p| p.title.clone());
            match s {
                State::NeedsYou => {
                    self.emit("agent.needs-you", json!({ "pane": id, "title": title }))
                }
                State::Done => self.emit("agent.done", json!({ "pane": id, "title": title })),
                _ => {}
            }
            if matches!(s, State::NeedsYou | State::Done) && focused != Some(id) {
                self.alert(id, s);
            }
        }
        self.check_waiters();
    }

    fn alert(&mut self, id: u64, s: State) {
        let name = self
            .panes
            .get(&id)
            .map(|p| p.title.clone())
            .unwrap_or_default();
        let text = match s {
            State::NeedsYou => format!("{name} needs you  (Ctrl+Shift+J)"),
            _ => format!("{name} is done  (Ctrl+Shift+J)"),
        };
        if self.cfg.notify.popup {
            self.toast(text, if s == State::NeedsYou { ALERT } else { DONE });
        }
        if self.cfg.notify.sound && !platform::in_lightsout() {
            let file = if s == State::NeedsYou {
                &self.cfg.notify.sound_needs_you
            } else {
                &self.cfg.notify.sound_done
            };
            platform::sound(file, s == State::NeedsYou);
        }
    }

    fn check_waiters(&mut self) {
        let now = Instant::now();
        let panes = &self.panes;
        self.waiters.retain_mut(|w| {
            let state = panes.get(&w.pane).map(|p| p.det.state);
            if matches!(state, Some(State::Working | State::NeedsYou | State::Done)) {
                w.seen_busy = true;
            }
            let idle_counts = w.seen_busy || w.since.elapsed() > Duration::from_secs(15);
            match state {
                None => {
                    let _ = w.reply.send(json!({ "state": "closed" }));
                    false
                }
                Some(s) if w.until.contains(&s) && (s != State::Idle || idle_counts) => {
                    let _ = w.reply.send(json!({ "state": s.name() }));
                    false
                }
                Some(s) if now >= w.deadline => {
                    let _ = w.reply.send(json!({ "state": s.name(), "timeout": true }));
                    false
                }
                _ => true,
            }
        });
    }

    // ------------------------------------------------------------ sessions

    fn saved(&self, screen_lines: usize) -> Saved {
        let running = &self.commands;
        let rooms = self
            .rooms
            .iter()
            .enumerate()
            .map(|(ri, r)| {
                let (tabs, active) = if ri == self.room {
                    (&self.tabs, self.active)
                } else {
                    (&r.tabs, r.active)
                };
                session::SavedRoom {
                    name: r.name.clone(),
                    // a room on another machine keeps its tabs there
                    tabs: if r.remote.is_some() {
                        Vec::new()
                    } else {
                        self.saved_tabs(tabs, running, screen_lines)
                    },
                    active,
                    remote: r.remote.clone(),
                }
            })
            .collect();
        Saved {
            tabs: Vec::new(),
            active: 0,
            rooms,
            room: self.room,
        }
    }

    fn saved_tabs(
        &self,
        tabs: &[TabView],
        running: &HashMap<u32, Option<String>>,
        screen_lines: usize,
    ) -> Vec<SavedTab> {
        tabs.iter()
            .map(|t| {
                let ids = t.panes();
                let index = |id: u64| ids.iter().position(|x| *x == id).unwrap_or(0) as u64;
                let panes = ids
                    .iter()
                    .filter_map(|id| self.panes.get(id))
                    // everything as it is now, asked of the system: the folder the tab's
                    // program (or its shell) works in, the program, the agent's conversation
                    // (or as last read: Claude removes its record when it stops)
                    .map(|p| SavedPane {
                        cwd: p
                            .running_pid
                            .or(p.shell_pid)
                            .and_then(platform::process_cwd)
                            .unwrap_or_else(|| p.cwd.clone())
                            .to_string_lossy()
                            .to_string(),
                        command: p
                            .shell_pid
                            .and_then(|pid| running.get(&pid).cloned().flatten())
                            .or_else(|| p.command.clone()),
                        session: p
                            .running_pid
                            .and_then(platform::agent_session)
                            .or_else(|| p.agent_session.clone()),
                        screen: (screen_lines > 0).then(|| p.text(screen_lines)),
                    })
                    .collect();
                SavedTab {
                    layout: t.layout.remap(&index),
                    panes,
                    focus: index(t.focus) as usize,
                    floats: t
                        .floats
                        .iter()
                        .map(|f| layout::Float {
                            id: index(f.id),
                            ..f.clone()
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// Quit, Lights On, or Windows restarting into Lights Out: the tabs are saved now, once.
    /// From here nothing saves or closes tabs: the agents are stopping, and as they stop their
    /// tabs would close (the last one even removes the saved tabs) and a later save would no
    /// longer know which conversation each tab had.
    fn begin_closing(&mut self) {
        self.save_session();
        self.closing = true;
    }

    fn save_session(&mut self) {
        if self.closing {
            return;
        }
        self.saved_at = Instant::now();
        if !self.cfg.history.keep
            || (self.tabs.is_empty() && self.rooms.iter().all(|r| r.tabs.is_empty()))
        {
            return;
        }
        self.saved_shape = self.shape();
        let s = self.saved(200);
        session::save(&session::local_file(&self.session), &s);
    }

    /// What a restart brings back, without the screens (cheap to compare).
    fn shape(&self) -> String {
        serde_json::to_string(&self.saved(0)).unwrap_or_default()
    }

    /// Known agents (resume.conf) continue their conversation; any other command is
    /// typed back in, unrun.
    fn restore(&mut self, saved: Saved, from_windows: bool) {
        let mut rooms = saved.rooms;
        if rooms.is_empty() && !saved.tabs.is_empty() {
            rooms.push(session::SavedRoom {
                name: "Room 1".into(),
                tabs: saved.tabs,
                active: saved.active,
                remote: None,
            });
        }
        if rooms.is_empty() {
            return;
        }
        self.rooms.clear();
        self.tabs.clear();
        for (ri, r) in rooms.iter().enumerate() {
            if let Some(remote) = &r.remote {
                self.open_remote(remote);
            } else {
                self.restore_tabs(&r.tabs, from_windows, ri);
            }
            let tabs = std::mem::take(&mut self.tabs);
            if !tabs.is_empty() {
                self.rooms.push(Room {
                    name: r.name.clone(),
                    active: r.active.min(tabs.len() - 1),
                    tabs,
                    remote: r.remote.clone(),
                    ..Default::default()
                });
            }
        }
        if self.rooms.is_empty() {
            self.rooms.push(Room {
                name: "Room 1".into(),
                tabs: Vec::new(),
                active: 0,
                ..Default::default()
            });
        }
        self.room = saved.room.min(self.rooms.len() - 1);
        self.tabs = std::mem::take(&mut self.rooms[self.room].tabs);
        self.active = self.rooms[self.room]
            .active
            .min(self.tabs.len().saturating_sub(1));
        self.resize_all();
    }

    fn restore_tabs(&mut self, saved_tabs: &[SavedTab], from_windows: bool, room: usize) {
        for (ti, t) in saved_tabs.iter().enumerate() {
            let mut ids = Vec::new();
            for (n, p) in t.panes.iter().enumerate() {
                let cwd = if from_windows {
                    platform::map_path(&p.cwd)
                } else {
                    PathBuf::from(&p.cwd)
                };
                // an agent's own conversation continues (resume.conf); any other command, or an
                // agent whose conversation isn't known, is typed back in
                let resume = p
                    .command
                    .as_deref()
                    .and_then(|c| platform::resume_command(c, p.session.as_deref()));
                let typed = if resume.is_none() {
                    p.command.clone()
                } else {
                    None
                };
                let mut env = Vec::new();
                if let Some(screen) = screen_to_show(p.screen.as_deref(), resume.as_deref()) {
                    let f =
                        std::env::temp_dir().join(format!("lightsout-screen-{room}-{ti}-{n}.txt"));
                    if std::fs::write(&f, screen).is_ok() {
                        env.push((
                            "LIGHTSOUT_SCREEN".to_string(),
                            f.to_string_lossy().to_string(),
                        ));
                    }
                }
                let size = self.grid_size(self.content());
                let id = self.spawn(cwd, resume, env, size);
                if let (Some(id), Some(cmd)) = (id, typed) {
                    if let Some(pane) = self.panes.get(&id) {
                        pane.write_str(cmd);
                    }
                }
                ids.push(id);
            }
            let Some(first) = ids.iter().flatten().next().copied() else {
                continue;
            };
            let map = |i: u64| ids.get(i as usize).copied().flatten().unwrap_or(first);
            let layout = t.layout.remap(&map);
            let focus = ids.get(t.focus).copied().flatten().unwrap_or(first);
            // floating panes come back where they were (one that didn't start is left out)
            let floats = t
                .floats
                .iter()
                .filter_map(|f| {
                    let id = ids.get(f.id as usize).copied().flatten()?;
                    Some(layout::Float { id, ..f.clone() })
                })
                .collect();
            self.tabs.push(TabView {
                layout,
                focus,
                floats,
            });
        }
    }

    fn start(&mut self) {
        if let Some(s) = platform::take_handoff() {
            // the tabs saved at a switch are the newest: in Lights Out the ones from Windows
            // (their paths mapped); back on the Linux desktop / logged back in on the Mac; or a
            // Windows switch that didn't reach Lights Out
            self.restore(s, platform::image());
        } else if let Some(s) = platform::take_handback() {
            // back in Windows: the tabs from Lights Out come along (conversations continue)
            self.restore(s, true);
        } else if self.cfg.history.keep {
            // also a terminal that crashed or was closed with Ctrl+Alt+Backspace in Lights Out
            if let Some(s) = session::load(&session::local_file(&self.session)) {
                self.restore(s, false);
            }
        }
        if platform::drive_read_only() {
            self.toast("C: is read-only: Windows was hibernated. Shut Windows down fully (or restart) before Lights Out to save files.".into(), ALERT);
        }
        // agents that tell their conversation through a hook get it (an agent installed since)
        platform::agent_hooks(true);
        if self.tabs.is_empty() {
            let start = std::env::current_dir()
                .ok()
                .filter(|d| d.is_dir())
                .unwrap_or_else(platform::home);
            self.new_tab(start, None, Vec::new());
        }
        self.sync_server_rooms();
        self.start_bar_items();
        self.started = true;
        self.start_parts();
        self.tell_part_sizes();
        self.emit("app.started", json!({}));
    }

    fn start_bar_items(&mut self) {
        self.bar_texts = self
            .cfg
            .bar
            .items
            .iter()
            .map(|i| i.text.clone().unwrap_or_default())
            .collect();
        for (n, item) in self.cfg.bar.items.iter().enumerate() {
            let Some(cmd) = item.command.clone() else {
                continue;
            };
            let (proxy, every) = (self.proxy.clone(), item.every.max(5));
            std::thread::spawn(move || loop {
                let (sh, flag) = if cfg!(windows) {
                    ("cmd", "/C")
                } else {
                    ("sh", "-c")
                };
                let out = platform::quiet(sh).args([flag, &cmd]).output();
                let text = out
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(40)
                            .collect()
                    })
                    .unwrap_or_default();
                if !proxy.send(UserEvent::BarText(n, text)) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(every));
            });
        }
    }

    // ------------------------------------------------------------ switching

    /// Quit: the tabs saved (once), the agents stopped, the app closes.
    fn quit_saved(&mut self) {
        self.begin_closing();
        for p in self.panes.values() {
            p.shutdown();
        }
        self.quit = true;
    }

    /// Lights On: back to Windows or the desktop. Everywhere but the Mac (where this window
    /// stays) the system restarts or the Lights Out screen ends, so the tabs are kept now.
    fn lights_on(&mut self) {
        if !cfg!(target_os = "macos") {
            self.begin_closing();
        }
        self.emit("lights.on", json!({}));
        platform::back_to_windows(&self.saved(80));
    }

    fn start_switch(&mut self) {
        self.emit("lights.out", json!({}));
        let saved = self.saved(80);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(platform::enter_lightsout(&saved));
        });
        self.switching = Some(rx);
        self.toast(
            "Switching to Lights Out... saving your tabs".into(),
            self.theme.accent,
        );
    }

    // ------------------------------------------------------------ actions

    /// The keys in use: yours from the settings, your plugins', then the defaults (see
    /// commands.rs).
    fn keymap(&self) -> Vec<(String, String)> {
        let (own, from_plugins) = self.key_sources();
        commands::keymap(&own, &from_plugins)
    }

    /// Your keys (key, command) and your plugins' (plugin, key, command), in the order they
    /// were added.
    #[allow(clippy::type_complexity)]
    fn key_sources(&self) -> (Vec<(String, String)>, Vec<(String, String, String)>) {
        let own = self
            .cfg
            .keys
            .iter()
            .map(|(k, b)| (k.clone(), b.command()))
            .collect();
        let from_plugins = self
            .room_plugins()
            .flat_map(|p| {
                p.m.keys
                    .iter()
                    .map(|k| (p.m.id.clone(), k.key.clone(), k.command.clone()))
            })
            .collect();
        (own, from_plugins)
    }

    /// A plugin key someone else had first (you, set later; or a plugin added before) does
    /// nothing: said once, in a popup.
    fn warn_skipped_keys(&mut self) {
        let (own, from_plugins) = self.key_sources();
        let now: Vec<String> = commands::bindings(&own, &from_plugins)
            .1
            .iter()
            .map(|s| {
                format!(
                    "{}'s key {} does nothing: {} has it ({})",
                    s.plugin,
                    s.key,
                    s.taken.owner.name(),
                    s.taken.command
                )
            })
            .collect();
        for w in now.iter().filter(|w| !self.keys_warned.contains(w)) {
            self.toasts.push(Toast {
                text: w.clone(),
                color: ALERT,
                at: Instant::now(),
            });
        }
        self.keys_warned = now;
        self.dirty = true;
    }

    // ------------------------------------------------------------ plugins

    /// Reads the plugins that are on (at start, and when plugins.json or a plugin's
    /// description changes).
    fn load_plugins(&mut self) {
        self.plugins_stamp = plugins::stamp();
        if self.plain {
            self.plugins.clear();
            self.part_content.clear();
            self.part_images.clear();
            self.part_sizes.clear();
            self.parts_started.clear();
            self.resize_all();
            return;
        }
        let (list, problems) = plugins::load();
        self.plugins = list;
        for p in problems {
            self.toast(p, ALERT);
        }
        self.warn_skipped_keys();
        if self.started {
            self.start_parts();
        }
        self.resize_all();
        self.dirty = true;
    }

    /// The programs of plugin parts that just appeared (each once), with the part's size;
    /// what parts that went away showed is forgotten.
    fn start_parts(&mut self) {
        let layout = self.screen_layout();
        let present: Vec<String> = self
            .plugins
            .iter()
            .flat_map(|p| p.m.parts.iter().map(|x| format!("{}.{}", p.m.id, x.id)))
            .collect();
        self.parts_started.retain(|id| present.contains(id));
        self.part_content.retain(|id, _| present.contains(id));
        self.part_images.retain(|id, _| present.contains(id));
        self.part_sizes.retain(|id, _| present.contains(id));
        let (cw, ch) = if self.tui {
            (1, 1)
        } else {
            (
                self.fonts.cell_w.max(1) as i32,
                self.fonts.cell_h.max(1) as i32,
            )
        };
        let due: Vec<(plugins::Plugin, String, Vec<String>)> = self
            .plugins
            .iter()
            .flat_map(|p| {
                p.m.parts
                    .iter()
                    .filter(|x| !x.run.is_empty() && plugins::runs_here(&x.platforms))
                    .map(|x| (p.clone(), format!("{}.{}", p.m.id, x.id), x.run.clone()))
            })
            .filter(|(_, id, _)| !self.parts_started.contains(id))
            .collect();
        for (p, id, run) in due {
            self.parts_started.push(id.clone());
            let r = layout
                .parts
                .iter()
                .find(|(x, _, _)| *x == id)
                .map(|(_, r, _)| *r)
                .unwrap_or_default();
            let (pw, ph) = self.part_pixels(r);
            let extra = vec![
                ("LIGHTSOUT_PART", id),
                ("LIGHTSOUT_PART_COLS", (r.w / cw).to_string()),
                ("LIGHTSOUT_PART_ROWS", (r.h / ch).to_string()),
                ("LIGHTSOUT_PART_WIDTH", pw.to_string()),
                ("LIGHTSOUT_PART_HEIGHT", ph.to_string()),
            ];
            self.start_plugin(&p, &run, extra);
        }
    }

    /// Plugins changed on disk (installed, switched on or off, a description edited) are read
    /// again right before they are used: a key, the menu, `lightsout do`, an event. So a
    /// change applies at once, also in Lights Out with the screen off, and nothing has to
    /// keep checking (a few file dates, no timer).
    fn fresh_plugins(&mut self) {
        if !self.plain && plugins::stamp() != self.plugins_stamp {
            self.load_plugins();
        }
    }

    /// A command by its name: a built-in one, or one of a plugin's (`PLUGIN.COMMAND [ARG]`).
    fn parse_command(&self, line: &str) -> Option<Action> {
        if let Some(a) = Action::parse(line) {
            return Some(a);
        }
        let line = line.trim();
        let (name, arg) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let (plugin, command) = name.split_once('.')?;
        let p = self.room_plugins().find(|p| p.m.id == plugin)?;
        // (a command may come once per system, under the same name)
        let here =
            p.m.commands
                .iter()
                .any(|c| c.id == command && plugins::runs_here(&c.platforms));
        here.then(|| Action::Plugin {
            plugin: plugin.to_string(),
            command: command.to_string(),
            arg: arg.trim().to_string(),
        })
    }

    /// Where you are, for a plugin's program: session, room, tab, pane, its folder and
    /// program, the selected text.
    fn context(&self) -> Value {
        let pane = self.focused().and_then(|id| self.panes.get(&id));
        json!({
            "session": self.session,
            "room": self.rooms.get(self.room).map(|r| r.name.clone()),
            "tab": self.active + 1,
            "pane": pane.map(|p| p.id),
            "cwd": pane.map(|p| p.cwd.to_string_lossy().to_string()),
            "program": pane.and_then(|p| p.running.clone()),
            "selection": pane.and_then(|p| p.term.lock().selection_to_string()),
        })
    }

    /// Starts one of a plugin's programs with what it needs to know.
    fn start_plugin(&mut self, p: &plugins::Plugin, run: &[String], extra: Vec<(&str, String)>) {
        let mut env = vec![
            ("LIGHTSOUT_SESSION", self.session.clone()),
            ("LIGHTSOUT_PLUGIN_CONTEXT", self.context().to_string()),
            ("LIGHTSOUT_THEME", self.theme_json().to_string()),
        ];
        // `lightsout read self` and the like reach the pane you are in
        if let Some(id) = self.focused() {
            env.push(("LIGHTSOUT_PANE", id.to_string()));
        }
        env.extend(extra);
        if let Err(e) = plugins::spawn(p, run, &env) {
            self.toast(e, ALERT);
        }
    }

    /// Something happened: every plugin that asked for it runs its program.
    fn emit(&mut self, event: &str, data: Value) {
        self.fresh_plugins();
        let due: Vec<(plugins::Plugin, Vec<String>)> = self
            .room_plugins()
            .flat_map(|p| {
                p.m.events
                    .iter()
                    .filter(|e| e.on == event && plugins::runs_here(&e.platforms))
                    .map(|e| (p.clone(), e.run.clone()))
            })
            .collect();
        for (p, run) in due {
            let extra = vec![
                ("LIGHTSOUT_EVENT", event.to_string()),
                ("LIGHTSOUT_EVENT_JSON", data.to_string()),
            ];
            self.start_plugin(&p, &run, extra);
        }
    }

    /// Whether a command can run here (the rest of the time it does nothing, and its key
    /// goes to the program in the pane).
    fn usable(&self, a: &Action) -> bool {
        match a {
            Action::Detach => self.tui,
            _ => true,
        }
    }

    /// A command as a line of a menu: its title (some say what they do here) and its key.
    fn item(&self, id: &str, keys: &[(String, String)]) -> Option<Item> {
        let c = commands::COMMANDS.iter().find(|c| c.id == id)?;
        let lo = platform::in_lightsout();
        let label = match id {
            "room.close"
                if self
                    .rooms
                    .get(self.room)
                    .is_some_and(|r| r.remote.is_some()) =>
            {
                "Remove this room here (its agents keep running on the server)"
            }
            "lights.switch" if lo && platform::image() => "Lights On (back to Windows)",
            "lights.switch" if lo => "Lights On (back to the desktop)",
            "lights.switch" if cfg!(windows) => "Lights Out (Windows off, only this terminal)",
            "lights.switch" => "Lights Out (desktop off, only this terminal)",
            _ => c.title,
        };
        let hint = if c.hint.is_empty() {
            keys.iter()
                .find(|(_, cmd)| cmd == id)
                .map(|(k, _)| commands::key_label(k))
                .unwrap_or_default()
        } else {
            c.hint.to_string()
        };
        Some(Item {
            label: label.into(),
            hint,
            action: Action::parse(id)?,
        })
    }

    /// The actions menu: every command that makes sense here, your plugins' commands, the
    /// other rooms, and the command lines your own keys run.
    fn actions(&self) -> Vec<Item> {
        let lo = platform::in_lightsout();
        let keys = self.keymap();
        let shown = |id: &str| match id {
            "wifi.choose" => platform::wifi_available(),
            "screen.off" => lo,
            "app.detach" => !lo && self.tui,
            "app.quit" => !lo,
            // inside a terminal, the terminal sets the font
            "font.bigger" | "font.smaller" => !self.tui,
            _ => true,
        };
        let mut v: Vec<Item> = commands::COMMANDS
            .iter()
            .filter(|c| c.menu && shown(c.id))
            .filter_map(|c| self.item(c.id, &keys))
            .collect();
        for p in self.room_plugins() {
            for c in
                p.m.commands
                    .iter()
                    .filter(|c| c.menu && plugins::runs_here(&c.platforms))
            {
                let name = format!("{}.{}", p.m.id, c.id);
                v.push(Item {
                    label: c.title.clone(),
                    // its key, else which plugin it comes from
                    hint: keys
                        .iter()
                        .find(|(_, cmd)| *cmd == name)
                        .map(|(k, _)| commands::key_label(k))
                        .unwrap_or_else(|| p.m.name.clone()),
                    action: Action::Plugin {
                        plugin: p.m.id.clone(),
                        command: c.id.clone(),
                        arg: String::new(),
                    },
                });
            }
        }
        for (i, r) in self.rooms.iter().enumerate() {
            if i != self.room {
                v.push(Item {
                    label: format!("Go to {}", r.name),
                    hint: "room".into(),
                    action: Action::GoRoom(i),
                });
            }
        }
        for (k, cmd) in &keys {
            if let Some(Action::Run { mode, command }) = Action::parse(cmd) {
                v.push(Item {
                    label: format!("Run: {command}"),
                    hint: commands::key_label(k),
                    action: Action::Run { mode, command },
                });
            }
        }
        v
    }

    fn agent_items(&self) -> Vec<Item> {
        // the agents in resume.conf (data), plus "any command"
        let mut v: Vec<Item> = platform::agents()
            .iter()
            .map(|a| Item {
                label: a.program.clone(),
                hint: "new tab".into(),
                action: Action::Run {
                    mode: "tab",
                    command: a.program.clone(),
                },
            })
            .collect();
        v.push(Item {
            label: "Any other command...".into(),
            hint: "type it".into(),
            action: Action::AskCommand,
        });
        v
    }

    fn join_wifi(&mut self, i: usize, password: Option<String>) {
        let Some(ssid) = self.wifi_nets.get(i).map(|n| n.ssid.clone()) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let name = ssid.clone();
        std::thread::spawn(move || {
            let _ = tx.send(platform::wifi_join(&name, password.as_deref()).map(|_| name));
        });
        self.wifi_join = Some(rx);
        self.toast(format!("Joining {ssid}..."), DIMTEXT);
    }

    fn open_palette(&mut self) {
        self.fresh_plugins();
        self.overlay = Overlay::Palette {
            title: "Everything Lights Out can do".into(),
            items: self.actions(),
            filter: String::new(),
            sel: 0,
            picked: false,
            top: 0,
        };
        self.dirty = true;
    }

    /// Does one command (from the menu, a key, a button, `lightsout do` or the API).
    fn run(&mut self, a: Action) {
        let cwd = self.cwd();
        match a {
            Action::Palette => self.open_palette(),
            Action::Agents => {
                self.overlay = Overlay::Palette {
                    title: "Start an agent in a new tab".into(),
                    items: self.agent_items(),
                    filter: String::new(),
                    sel: 0,
                    picked: false,
                    top: 0,
                };
            }
            Action::NewTab => {
                self.new_tab(cwd, None, Vec::new());
            }
            Action::AskCommand => {
                self.overlay = Overlay::Ask {
                    title: "Command to run in a new tab:".into(),
                    text: String::new(),
                    purpose: Purpose::Command,
                }
            }
            Action::NewRoom => self.new_room(None),
            Action::NewServerRoom => {
                self.overlay = Overlay::Ask {
                    title: "New room on a server (user@host, or a name from your ssh config):"
                        .into(),
                    text: String::new(),
                    purpose: Purpose::ServerRoom,
                }
            }
            Action::RenameRoom => {
                let text = self.rooms[self.room].name.clone();
                self.overlay = Overlay::Ask {
                    title: "Room name:".into(),
                    text,
                    purpose: Purpose::RoomName(self.room),
                };
            }
            Action::CloseRoom => self.close_room(),
            Action::GoRoom(i) => self.switch_room(i),
            Action::SplitRight => {
                self.split(true, None, None, Vec::new());
            }
            Action::SplitDown => {
                self.split(false, None, None, Vec::new());
            }
            Action::ClosePane => {
                if let Some(id) = self.focused() {
                    self.close_pane(id);
                }
            }
            // floating panes (a window manager plugin uses these with a pane number)
            Action::FloatPane(p) => {
                if let Some((ti, id)) = self.target_pane(p) {
                    self.tabs[ti].float_pane(id);
                    self.tabs[ti].focus = id;
                    self.resize_all();
                }
            }
            Action::TilePane(p) => {
                if let Some((ti, id)) = self.target_pane(p) {
                    self.tabs[ti].tile_pane(id);
                    self.resize_all();
                }
            }
            Action::MovePane { x, y, pane } => {
                if let Some((ti, id)) = self.target_pane(pane) {
                    self.tabs[ti].float_pane(id);
                    if let Some(f) = self.tabs[ti].float(id) {
                        (f.x, f.y) = (x as f32 / 100.0, y as f32 / 100.0);
                    }
                    self.resize_all();
                }
            }
            Action::ResizePane { w, h, pane } => {
                if let Some((ti, id)) = self.target_pane(pane) {
                    self.tabs[ti].float_pane(id);
                    if let Some(f) = self.tabs[ti].float(id) {
                        (f.w, f.h) = (w as f32 / 100.0, h as f32 / 100.0);
                    }
                    self.resize_all();
                }
            }
            Action::MinimizePane(p) => {
                if let Some((ti, id)) = self.target_pane(p) {
                    self.tabs[ti].minimize(id);
                    self.resize_all();
                }
            }
            Action::RaisePane(p) => {
                if let Some((ti, id)) = self.target_pane(p) {
                    self.tabs[ti].raise(id);
                    self.focus(id);
                }
            }
            Action::Focus(dx, dy) => {
                if let Some(from) = self.tabs.get(self.active).map(|t| t.focus) {
                    if let Some(to) = layout::neighbor(&self.active_rects(), from, dx, dy) {
                        self.focus(to);
                    }
                }
            }
            Action::NextTab => self.cycle(1),
            Action::PrevTab => self.cycle(-1),
            Action::GoTab(i) => {
                if let Some(f) = self.tabs.get(i).map(|t| t.focus) {
                    self.focus(f);
                }
            }
            Action::Jump => self.jump(),
            Action::Editor => self.open_in_editor(),
            Action::Copy => self.copy(),
            Action::Paste => self.paste(),
            Action::ScrollUp | Action::ScrollDown => {
                let rows = self
                    .focused()
                    .and_then(|id| self.panes.get(&id))
                    .map(|p| p.size.rows as i32)
                    .unwrap_or(20);
                self.scroll(if a == Action::ScrollUp {
                    rows / 2
                } else {
                    -rows / 2
                });
            }
            Action::Detach => {
                self.detach = true;
                self.dirty = true;
            }
            Action::Lights => {
                if platform::in_lightsout() {
                    self.overlay = Overlay::Confirm(Confirm::BackToWindows);
                } else {
                    self.overlay = Overlay::Confirm(Confirm::EnterLightsOut);
                }
            }
            Action::Wifi => {
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(platform::wifi_scan());
                });
                self.wifi_scan = Some(rx);
                self.toast("Looking for Wi-Fi networks...".into(), DIMTEXT);
            }
            Action::WifiJoin(i) => {
                if let Some(n) = self.wifi_nets.get(i) {
                    if n.connected {
                        self.toast(format!("Already on {}", n.ssid), DIMTEXT);
                    } else if n.secure {
                        let title = format!("Password for {}:", n.ssid);
                        self.overlay = Overlay::Ask {
                            title,
                            text: String::new(),
                            purpose: Purpose::WifiPassword(i),
                        };
                    } else {
                        self.join_wifi(i, None);
                    }
                }
            }
            Action::WifiOther => {
                self.overlay = Overlay::Ask {
                    title: "Network name:".into(),
                    text: String::new(),
                    purpose: Purpose::WifiName,
                };
            }
            Action::ScreenOff => {
                platform::screen(false);
                // macOS wakes the display itself (lid, trackpad), so keep drawing there
                self.screen_off = !cfg!(target_os = "macos");
                self.emit("screen.off", json!({}));
            }
            // in Lights Out, quitting is going back (the app has to keep running there)
            Action::Quit if platform::in_lightsout() => {
                self.overlay = Overlay::Confirm(Confirm::BackToWindows)
            }
            Action::Quit => self.overlay = Overlay::Confirm(Confirm::Quit),
            Action::Plain => {
                self.plain = !self.plain;
                self.load_plugins();
                self.toast(
                    if self.plain {
                        "Plugins off (Ctrl+Shift+F12 switches them back on)"
                    } else {
                        "Plugins on"
                    }
                    .into(),
                    ALERT,
                );
            }
            Action::Settings => {
                let _ = config::load(); // creates the file with explanations the first time
                if let Some(cmd) = platform::editor_command(&config::path()) {
                    self.new_tab(cwd, Some(cmd), Vec::new());
                }
            }
            Action::FontBigger => self.set_font(self.fonts.px + 1.0),
            Action::FontSmaller => self.set_font(self.fonts.px - 1.0),
            Action::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                save_ui(&UiState {
                    sidebar: self.sidebar,
                });
                self.resize_all();
            }
            Action::Run { mode, command } => self.open_in(mode, command, Vec::new()),
            Action::Plugin {
                plugin,
                command,
                arg,
            } => {
                let found = self
                    .room_plugins()
                    .find(|p| p.m.id == plugin)
                    .and_then(|p| {
                        let c =
                            p.m.commands
                                .iter()
                                .find(|c| c.id == command && plugins::runs_here(&c.platforms))?;
                        Some((p.clone(), c.run.clone(), c.place.clone()))
                    });
                let name = format!("{plugin}.{command}");
                match found {
                    Some((p, run, place)) if place == "background" => {
                        let extra = vec![("LIGHTSOUT_COMMAND", name), ("LIGHTSOUT_ARG", arg)];
                        self.start_plugin(&p, &run, extra);
                    }
                    // on screen: its program in a pane, in your folder, with what it needs
                    Some((p, run, place)) => {
                        let line = plugins::argv(&p, &run)
                            .iter()
                            .map(|w| shell_word(w))
                            .collect::<Vec<_>>()
                            .join(" ");
                        // PowerShell runs a quoted program only after `&`
                        let line = if cfg!(windows) {
                            format!("& {line}")
                        } else {
                            line
                        };
                        let mut env = plugins::env(&p);
                        env.push((
                            "LIGHTSOUT_PLUGIN_CONTEXT".into(),
                            self.context().to_string(),
                        ));
                        env.push(("LIGHTSOUT_THEME".into(), self.theme_json().to_string()));
                        env.push(("LIGHTSOUT_COMMAND".into(), name));
                        env.push(("LIGHTSOUT_ARG".into(), arg));
                        self.open_in(&place, line, env);
                    }
                    None => self.toast(format!("{name} is gone"), ALERT),
                }
            }
        }
        self.dirty = true;
    }

    /// A command line on screen: in a new tab, a split (right, or `split-down`), or a popup
    /// that closes when it ends.
    fn open_in(&mut self, place: &str, command: String, env: Vec<(String, String)>) {
        let cwd = self.cwd();
        match place {
            "tab" => {
                self.new_tab(cwd, Some(command), env);
            }
            "split" => {
                self.split(true, Some(command), None, env);
            }
            "split-down" => {
                self.split(false, Some(command), None, env);
            }
            _ => {
                let size = self.grid_size(self.popup_rect());
                let cmd = format!("{command}; exit");
                if let Some(id) = self.spawn(cwd, Some(cmd), env, size) {
                    self.popup = Some(id);
                }
            }
        }
    }

    fn cycle(&mut self, d: i32) {
        if !self.tabs.is_empty() {
            let n = self.tabs.len() as i32;
            self.active = ((self.active as i32 + d).rem_euclid(n)) as usize;
            let f = self.tabs[self.active].focus;
            self.focus(f);
        }
    }

    fn set_font(&mut self, px: f32) {
        self.fonts.set_size(px);
        self.resize_all();
    }

    fn copy(&mut self) {
        let text = self
            .focused()
            .and_then(|id| self.panes.get(&id))
            .and_then(|p| p.term.lock().selection_to_string());
        if let (Some(t), Some(c)) = (text, self.clipboard.as_mut()) {
            let _ = c.set_text(t);
            self.toast("Copied".into(), DIMTEXT);
        }
    }

    fn paste(&mut self) {
        let Some(text) = self.clipboard.as_mut().and_then(|c| c.get_text().ok()) else {
            return;
        };
        self.paste_text(text);
    }

    /// Pasted text goes to the open question or list filter, else to the focused pane.
    fn paste_text(&mut self, text: String) {
        match &mut self.overlay {
            Overlay::Ask { text: t, .. } | Overlay::Palette { filter: t, .. } => {
                t.push_str(text.lines().next().unwrap_or_default());
                self.dirty = true;
                return;
            }
            Overlay::None => {}
            _ => return,
        }
        let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) else {
            return;
        };
        let bracketed = p.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        p.input(
            if bracketed {
                format!("\x1b[200~{text}\x1b[201~")
            } else {
                text
            }
            .into_bytes(),
        );
    }

    fn open_in_editor(&mut self) {
        let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) else {
            return;
        };
        let file = std::env::temp_dir().join(format!("lightsout-output-{}.txt", p.id));
        let (text, cwd) = (p.text(100_000), p.cwd.clone());
        if std::fs::write(&file, text).is_ok() {
            if let Some(cmd) = platform::editor_command(&file) {
                self.new_tab(cwd, Some(cmd), Vec::new());
            }
        }
    }

    // ------------------------------------------------------------ keyboard

    fn key_name(&self, key: &Key) -> Option<String> {
        let m = self.mods;
        let base = match key {
            Key::Character(s) => s.to_lowercase(),
            Key::Named(n) => format!("{n:?}").to_lowercase(),
            _ => return None,
        };
        let mut parts = Vec::new();
        if m.control_key() {
            parts.push("ctrl");
        }
        if m.alt_key() {
            parts.push("alt");
        }
        if m.shift_key() {
            parts.push("shift");
        }
        parts.push(&base);
        Some(parts.join("+"))
    }

    /// A key that runs a command (yours from the settings, else the default one) runs it and
    /// answers true. Any other key (or one whose command can't run here) is the pane's.
    fn shortcut(&mut self, key: &Key) -> bool {
        let Some(name) = self.key_name(key) else {
            return false;
        };
        // the way back, before any key you or a plugin may have set: plugins off, or on again
        if name == commands::SAFE_KEY {
            self.run(Action::Plain);
            return true;
        }
        self.fresh_plugins();
        let Some((_, cmd)) = self.keymap().into_iter().find(|(k, _)| *k == name) else {
            return false;
        };
        match self.parse_command(&cmd) {
            Some(a) if self.usable(&a) => {
                self.run(a);
                true
            }
            _ => false,
        }
    }

    fn scroll(&mut self, lines: i32) {
        if let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) {
            p.scroll(lines);
            self.dirty = true;
        }
    }

    /// Keys while a palette, menu, question or confirmation is open.
    fn overlay_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if matches!(self.overlay, Overlay::None) {
            return false;
        }
        self.dirty = true;
        if let Overlay::Confirm(c) = self.overlay {
            self.overlay = Overlay::None;
            if matches!(key, Key::Named(NamedKey::Enter)) {
                match c {
                    Confirm::BackToWindows => {
                        self.lights_on();
                        if cfg!(target_os = "macos") {
                            if let Some(w) = &self.window {
                                w.set_fullscreen(None);
                            }
                        }
                    }
                    Confirm::EnterLightsOut => self.start_switch(),
                    Confirm::Quit => self.quit_saved(),
                }
            }
            return true;
        }
        if let Overlay::Ask {
            text: t, purpose, ..
        } = &mut self.overlay
        {
            match key {
                Key::Named(NamedKey::Escape) => self.overlay = Overlay::None,
                Key::Named(NamedKey::Enter) => {
                    let value = t.trim().to_string();
                    let purpose = *purpose;
                    self.overlay = Overlay::None;
                    if !value.is_empty() {
                        match purpose {
                            Purpose::Command => {
                                let cwd = self.cwd();
                                self.new_tab(cwd, Some(value), Vec::new());
                            }
                            Purpose::RoomName(i) => {
                                let before = self
                                    .rooms
                                    .get_mut(i)
                                    .map(|r| std::mem::replace(&mut r.name, value.clone()));
                                if let Some(before) = before {
                                    self.emit(
                                        "room.renamed",
                                        json!({ "room": value, "before": before }),
                                    );
                                }
                            }
                            Purpose::ServerRoom => {
                                let name = session::server_name(&value);
                                let mut all = session::server_rooms();
                                all.retain(|m| m.name != name);
                                all.push(session::ServerRoom {
                                    name: name.clone(),
                                    ssh: value,
                                });
                                if session::save_server_rooms(&all).is_err() {
                                    self.toast("Couldn't save that room".into(), ALERT);
                                }
                                self.sync_server_rooms();
                                if let Some(i) = self.rooms.iter().position(|r| {
                                    r.remote.as_ref().is_some_and(|x| x.server == name)
                                }) {
                                    self.switch_room(i);
                                }
                            }
                            Purpose::WifiPassword(i) => self.join_wifi(i, Some(value)),
                            Purpose::WifiName => {
                                // a network that isn't listed (hidden, or a name macOS doesn't show)
                                self.wifi_nets.push(platform::Net {
                                    ssid: value.clone(),
                                    secure: true,
                                    signal: 0,
                                    connected: false,
                                });
                                let i = self.wifi_nets.len() - 1;
                                self.overlay = Overlay::Ask {
                                    title: format!("Password for {value} (Esc if none):"),
                                    text: String::new(),
                                    purpose: Purpose::WifiPassword(i),
                                };
                            }
                        }
                    }
                }
                Key::Named(NamedKey::Backspace) => {
                    t.pop();
                }
                _ => {
                    if let Some(s) = text.filter(|s| !s.chars().any(char::is_control)) {
                        t.push_str(s);
                    }
                }
            }
            return true;
        }
        // palette or menu
        let visible = self.visible_items();
        match key {
            Key::Named(NamedKey::Escape) => self.overlay = Overlay::None,
            Key::Named(NamedKey::ArrowDown) => self.move_sel(1, visible.len()),
            Key::Named(NamedKey::ArrowUp) => self.move_sel(-1, visible.len()),
            Key::Named(NamedKey::Enter) => {
                // the chosen one; nothing chosen: the top match of what was typed
                let i = match &self.overlay {
                    Overlay::Palette {
                        picked: false,
                        filter,
                        ..
                    } if filter.is_empty() => return true,
                    Overlay::Palette { picked: false, .. } => 0,
                    _ => self.sel(),
                };
                let action = visible.get(i).cloned();
                self.overlay = Overlay::None;
                if let Some(a) = action {
                    self.run(a);
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if let Overlay::Palette {
                    filter,
                    sel,
                    picked,
                    top,
                    ..
                } = &mut self.overlay
                {
                    filter.pop();
                    (*sel, *picked, *top) = (0, false, 0);
                }
            }
            _ => {
                if let (
                    Some(s),
                    Overlay::Palette {
                        filter,
                        sel,
                        picked,
                        top,
                        ..
                    },
                ) = (text, &mut self.overlay)
                {
                    if !s.chars().any(char::is_control) {
                        filter.push_str(s);
                        (*sel, *picked, *top) = (0, false, 0);
                    }
                }
            }
        }
        true
    }

    fn visible_items(&self) -> Vec<Action> {
        match &self.overlay {
            Overlay::Palette { items, filter, .. } => {
                let f = filter.to_lowercase();
                items
                    .iter()
                    .filter(|i| f.is_empty() || i.label.to_lowercase().contains(&f))
                    .map(|i| i.action.clone())
                    .collect()
            }
            Overlay::Menu { items, .. } => items.iter().map(|i| i.action.clone()).collect(),
            _ => Vec::new(),
        }
    }

    fn sel(&self) -> usize {
        match &self.overlay {
            Overlay::Palette { sel, .. } | Overlay::Menu { sel, .. } => *sel,
            _ => 0,
        }
    }

    fn move_sel(&mut self, d: i32, n: usize) {
        let rows = self.palette_rows.max(1);
        if let Overlay::Palette {
            sel, picked, top, ..
        } = &mut self.overlay
        {
            if n == 0 {
                return;
            }
            if !*picked {
                // the first ↑/↓ chooses the first row in view
                *picked = true;
                *sel = (*top).min(n - 1);
            } else {
                *sel = ((*sel as i32 + d).rem_euclid(n as i32)) as usize;
            }
            if *sel < *top {
                *top = *sel;
            } else if *sel >= *top + rows {
                *top = *sel + 1 - rows;
            }
            return;
        }
        if let Overlay::Menu { sel, .. } = &mut self.overlay {
            if n > 0 {
                *sel = ((*sel as i32 + d).rem_euclid(n as i32)) as usize;
            }
        }
    }

    fn lock_key(&mut self, key: &Key) {
        let Lock::Locked { salt, hash, wrong } = &mut self.lock else {
            return;
        };
        match key {
            Key::Named(NamedKey::Enter) => {
                if platform::pin_matches(salt, hash, &self.pin) {
                    self.lock = Lock::Unlocked;
                } else {
                    *wrong += 1;
                    std::thread::sleep(Duration::from_millis(400 * (*wrong).min(10) as u64));
                }
                self.pin.clear();
            }
            Key::Named(NamedKey::Backspace) => {
                self.pin.pop();
            }
            Key::Character(s) => {
                if self.pin.len() < 32 {
                    self.pin.push_str(s);
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ mouse

    fn pane_at(&self, x: i32, y: i32) -> Option<(u64, Rect)> {
        if let Some(p) = self.popup {
            let r = self.popup_rect();
            if r.contains(x, y) {
                return Some((p, r));
            }
        }
        // the floating panes first, top one first (its title bar belongs to no pane)
        for (id, frame) in self.float_frames().into_iter().rev() {
            if frame.contains(x, y) {
                let body = self.float_body(frame);
                return body.contains(x, y).then_some((id, body));
            }
        }
        self.active_rects()
            .into_iter()
            .find(|(_, r)| r.contains(x, y))
    }

    fn cell_point(&self, pane: u64, r: Rect, x: i32, y: i32) -> Option<(Point, Side)> {
        let p = self.panes.get(&pane)?;
        let (cw, ch) = (self.fonts.cell_w as i32, self.fonts.cell_h as i32);
        let (padx, pady) = (self.padx(), self.pady());
        let col = ((x - r.x - padx).max(0) / cw).min(p.size.cols as i32 - 1);
        let row = ((y - r.y - pady).max(0) / ch).min(p.size.rows as i32 - 1);
        let offset = p.term.lock().grid().display_offset() as i32;
        let side = if !self.tui && (x - r.x - padx).rem_euclid(cw) > cw / 2 {
            Side::Right
        } else {
            Side::Left
        };
        Some((Point::new(Line(row - offset), Column(col as usize)), side))
    }

    /// A key was pressed (from the window, or from the terminal Lights Out runs in).
    pub(crate) fn key_press(&mut self, key: &Key, text: Option<&str>) {
        self.last_input = Instant::now();
        if self.screen_off {
            platform::screen(true);
            self.screen_off = false;
            self.dirty = true;
            return;
        }
        if matches!(self.lock, Lock::Locked { .. }) {
            self.lock_key(key);
            return;
        }
        if self.overlay_key(key, text) {
            return;
        }
        if self.shortcut(key) {
            return;
        }
        let Some(id) = self.focused() else { return };
        let mods = self.mods;
        if let Some(p) = self.panes.get_mut(&id) {
            p.det.seen();
            let app_cursor = {
                let mut term = p.term.lock();
                term.selection = None;
                term.mode().contains(TermMode::APP_CURSOR)
            };
            if let Some(bytes) = keys::to_bytes(key, text, mods, app_cursor) {
                p.input(bytes);
            }
        }
    }

    pub(crate) fn mouse_down(&mut self, button: MouseButton) {
        self.last_input = Instant::now();
        let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
        self.dirty = true;
        if let Overlay::Menu {
            x: mx,
            y: my,
            items,
            ..
        } = &self.overlay
        {
            // a click on a menu row runs it; anywhere else closes the menu
            let cw = self.fonts.cell_w as i32;
            let ch = self.fonts.cell_h as i32;
            let mw = items
                .iter()
                .map(|i| i.label.chars().count() + i.hint.chars().count() + 4)
                .max()
                .unwrap_or(10) as i32
                * cw;
            let inset = self.menu_inset();
            let row = (y - *my - inset) / ch;
            let action = (x >= *mx && x < *mx + mw && y >= *my + inset && row >= 0)
                .then(|| items.get(row as usize).map(|i| i.action.clone()))
                .flatten();
            self.overlay = Overlay::None;
            if let Some(a) = action {
                self.run(a);
            }
            return;
        }
        if matches!(self.overlay, Overlay::Palette { .. }) {
            let hit = self
                .hits
                .iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|h| h.1);
            match hit {
                Some(Hit::PaletteItem(n)) => {
                    let action = self.visible_items().get(n).cloned();
                    self.overlay = Overlay::None;
                    if let Some(a) = action {
                        self.run(a);
                    }
                }
                Some(Hit::PaletteBox) => {}
                _ => self.overlay = Overlay::None,
            }
            return;
        }
        if !matches!(self.overlay, Overlay::None) {
            return;
        }
        let layout = self.screen_layout();
        // a plugin part: the piece clicked runs its command; anywhere else, its plugin hears
        // where (part.clicked: a frame can be clicked anywhere)
        if let Some((part, r, _)) = layout.parts.iter().find(|(_, r, _)| r.contains(x, y)) {
            let piece = self.hits.iter().find(|(h, k)| {
                r.contains(h.x, h.y) && h.contains(x, y) && matches!(k, Hit::Part(_))
            });
            if let Some((_, Hit::Part(k))) = piece {
                let cmd = self.part_clicks.get(*k).cloned().unwrap_or_default();
                match self.parse_command(&cmd) {
                    Some(a) if self.usable(&a) => self.run(a),
                    _ => self.toast(
                        format!("unknown command `{cmd}` (lightsout commands)"),
                        ALERT,
                    ),
                }
            } else {
                let (cw, ch) = if self.tui {
                    (1, 1)
                } else {
                    (
                        self.fonts.cell_w.max(1) as i32,
                        self.fonts.cell_h.max(1) as i32,
                    )
                };
                let which = match button {
                    MouseButton::Right => "right",
                    MouseButton::Middle => "middle",
                    _ => "left",
                };
                // (a scrolled picture: `top` is how far down it is, so the place in it is y + top)
                let top = self.part_scroll.get(part).copied().unwrap_or(0);
                let data = json!({ "part": part, "x": x - r.x, "y": y - r.y, "col": (x - r.x) / cw, "row": (y - r.y) / ch, "button": which, "top": top });
                self.emit("part.clicked", data);
            }
            return;
        }
        if layout.sidebar.is_some_and(|r| r.contains(x, y)) {
            let hit = self
                .hits
                .iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|h| h.1);
            match hit {
                Some(Hit::Pane(id)) => self.focus(id),
                Some(Hit::Room(i)) => self.run(Action::GoRoom(i)),
                Some(Hit::NewRoom) => self.run(Action::NewRoom),
                Some(Hit::Palette) => self.run(Action::Palette),
                Some(Hit::Switch) => self.run(Action::Lights),
                _ => {}
            }
            return;
        }
        if layout.bar.is_some_and(|r| r.contains(x, y)) {
            let hit = self
                .hits
                .iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|h| h.1);
            match hit {
                Some(Hit::TabsLeft) => {
                    self.tabs_first = self.tabs_first.saturating_sub(1);
                    self.dirty = true;
                }
                Some(Hit::TabsRight) => {
                    self.tabs_first += 1;
                    self.dirty = true;
                }
                Some(Hit::Tab(i)) => self.run(Action::GoTab(i)),
                Some(Hit::NewTab) => self.run(Action::NewTab),
                Some(Hit::Palette) => self.run(Action::Palette),
                Some(Hit::Switch) => self.run(Action::Lights),
                _ => {}
            }
            return;
        }
        // a floating pane's title bar, buttons or corner (the top one's first)
        let float_hit = self
            .hits
            .iter()
            .rev()
            .find(|(r, h)| {
                r.contains(x, y)
                    && matches!(
                        h,
                        Hit::FloatTitle(_)
                            | Hit::FloatMin(_)
                            | Hit::FloatTile(_)
                            | Hit::FloatClose(_)
                            | Hit::FloatCorner(_)
                    )
            })
            .map(|h| (h.0, h.1));
        if let (Some((r, hit)), true, None) = (float_hit, button == MouseButton::Left, self.popup) {
            match hit {
                Hit::FloatTitle(id) => {
                    self.focus(id);
                    self.drag = Some(Drag::Float {
                        pane: id,
                        dx: x - r.x,
                        dy: y - r.y,
                    });
                }
                Hit::FloatCorner(id) => {
                    self.focus(id);
                    self.drag = Some(Drag::FloatSize { pane: id });
                }
                Hit::FloatMin(id) => self.run(Action::MinimizePane(Some(id))),
                Hit::FloatTile(id) => self.run(Action::TilePane(Some(id))),
                Hit::FloatClose(id) => self.close_pane(id),
                _ => {}
            }
            return;
        }
        let in_float = self.float_frames().iter().any(|(_, f)| f.contains(x, y));
        // dragging the line between two panes? (not through a floating one)
        if button == MouseButton::Left && self.popup.is_none() && !in_float {
            if let Some(t) = self.tabs.get(self.active) {
                let reach = if self.tui { 0 } else { 4 };
                for d in t.layout.layout(self.content()).1 {
                    let near = if d.side_by_side {
                        (x - d.at).abs() <= reach && d.area.contains(d.at, y)
                    } else {
                        (y - d.at).abs() <= reach && d.area.contains(x, d.at)
                    };
                    if near {
                        self.drag = Some(Drag::Divider {
                            tab: self.active,
                            path: d.path,
                            side_by_side: d.side_by_side,
                            area: d.area,
                        });
                        return;
                    }
                }
            }
        }
        let Some((pane, r)) = self.pane_at(x, y) else {
            return;
        };
        if self.popup != Some(pane) {
            self.focus(pane);
        }
        // Ctrl+click on a web link: a plugin's command for it, else the browser
        if button == MouseButton::Left && self.mods.control_key() {
            if let Some(link) = self.link_at(pane, r, x, y) {
                self.open_link(&link);
                return;
            }
        }
        // the program asked for the mouse (Shift keeps it for selecting and the menu)
        let code = match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            _ => 3,
        };
        if code < 3 && !self.mods.shift_key() && self.send_mouse(pane, r, x, y, code, true) {
            self.mouse_held = Some((pane, code));
            return;
        }
        match button {
            MouseButton::Left => {
                if let Some((pt, side)) = self.cell_point(pane, r, x, y) {
                    if let Some(p) = self.panes.get(&pane) {
                        p.term.lock().selection =
                            Some(Selection::new(SelectionType::Simple, pt, side));
                    }
                    self.drag = Some(Drag::Select { pane, origin: r });
                }
            }
            MouseButton::Right => {
                let keys = self.keymap();
                let items = [
                    "edit.copy",
                    "edit.paste",
                    "pane.split-right",
                    "pane.split-down",
                    "pane.output-in-editor",
                    "pane.close",
                ]
                .iter()
                .filter_map(|id| self.item(id, &keys))
                .collect();
                self.overlay = Overlay::Menu {
                    x,
                    y,
                    items,
                    sel: 0,
                };
            }
            _ => {}
        }
    }

    /// The web link under the mouse in a pane, if there is one.
    fn link_at(&self, pane: u64, r: Rect, x: i32, y: i32) -> Option<String> {
        let (pt, _) = self.cell_point(pane, r, x, y)?;
        let p = self.panes.get(&pane)?;
        let term = p.term.lock();
        let grid = term.grid();
        let line: Vec<char> = (0..p.size.cols)
            .map(|c| grid[pt.line][Column(c)].c)
            .collect();
        link_in(&line, pt.column.0)
    }

    /// A link was Ctrl+clicked: the first plugin whose pattern matches runs its command with
    /// the link; none: it opens in the browser.
    fn open_link(&mut self, link: &str) {
        self.fresh_plugins();
        let handler = self
            .room_plugins()
            .flat_map(|p| p.m.links.iter())
            .find(|l| plugins::matches(&l.pattern, link))
            .map(|l| l.command.clone());
        let action = handler.and_then(|c| match self.parse_command(&c) {
            // a plugin's command gets it as its argument (LIGHTSOUT_ARG), as it is
            Some(Action::Plugin {
                plugin, command, ..
            }) => Some(Action::Plugin {
                plugin,
                command,
                arg: link.to_string(),
            }),
            // any other command gets it after it, quoted (`run.popup gh pr view`)
            _ => self.parse_command(&format!("{c} {}", shell_word(link))),
        });
        match action {
            Some(a) if self.usable(&a) => self.run(a),
            _ => {
                if !platform::open_url(link) {
                    self.toast(format!("No browser here: {link}"), DIMTEXT);
                }
            }
        }
    }

    /// The mouse wheel over a list (actions, rooms, running, tabs) scrolls that list.
    /// `lines` > 0 is up. Returns false when the wheel is over none of them.
    fn wheel(&mut self, lines: i32) -> bool {
        if lines == 0 {
            return false;
        }
        let rows = self.palette_rows.max(1);
        if let Overlay::Palette {
            items, filter, top, ..
        } = &mut self.overlay
        {
            let f_l = filter.to_lowercase();
            let shown = items
                .iter()
                .filter(|i| f_l.is_empty() || i.label.to_lowercase().contains(&f_l))
                .count();
            let last = shown.saturating_sub(rows) as i32;
            *top = (*top as i32 - lines).clamp(0, last) as usize;
            self.dirty = true;
            return true;
        }
        let (mx, my) = (self.mouse.x as i32, self.mouse.y as i32);
        let Some(area) = self
            .scroll_areas
            .iter()
            .find(|(r, _)| r.contains(mx, my))
            .map(|a| a.1)
        else {
            return false;
        };
        let step = -lines.signum();
        let moved = |v: usize| (v as i32 + step).max(0) as usize;
        match area {
            Scroll::Rooms => self.rooms_top = moved(self.rooms_top),
            Scroll::Running => self.running_top = moved(self.running_top),
            Scroll::Tabs => self.tabs_first = moved(self.tabs_first),
        }
        self.dirty = true;
        true
    }

    /// Where a pane is on screen now.
    fn pane_rect(&self, pane: u64) -> Option<Rect> {
        if self.popup == Some(pane) {
            return Some(self.popup_rect());
        }
        self.active_rects()
            .into_iter()
            .find(|(id, _)| *id == pane)
            .map(|(_, r)| r)
    }

    /// Tells the pane's program about the mouse, if it asked for it (xterm reporting).
    fn send_mouse(&self, pane: u64, r: Rect, x: i32, y: i32, code: u8, press: bool) -> bool {
        let Some(p) = self.panes.get(&pane) else {
            return false;
        };
        let mode = *p.term.lock().mode();
        if !mode.intersects(TermMode::MOUSE_MODE) {
            return false;
        }
        let (cw, ch) = (self.fonts.cell_w as i32, self.fonts.cell_h as i32);
        let col = ((x - r.x - self.padx()).max(0) / cw + 1).min(p.size.cols as i32);
        let row = ((y - r.y - self.pady()).max(0) / ch + 1).min(p.size.rows as i32);
        let bytes = if mode.contains(TermMode::SGR_MOUSE) {
            format!("\x1b[<{code};{col};{row}{}", if press { 'M' } else { 'm' }).into_bytes()
        } else {
            if col > 222 || row > 222 {
                return true;
            }
            let b = if press { code } else { 3 };
            vec![0x1b, b'[', b'M', 32 + b, 32 + col as u8, 32 + row as u8]
        };
        p.input(bytes);
        true
    }

    /// The wheel: a list under the mouse scrolls; a plugin part hears it (part.scrolled);
    /// else the pane under it scrolls.
    fn on_wheel(&mut self, lines: i32) {
        if self.wheel(lines) {
            return;
        }
        let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
        if let Some((part, r, _)) = self
            .screen_layout()
            .parts
            .into_iter()
            .find(|(_, r, _)| r.contains(x, y))
        {
            // a picture taller than its part scrolls right here (nothing is drawn again)
            let (_, ph) = self.part_pixels(r);
            let room = self
                .part_images
                .get(&part)
                .map(|i| i.h as i32 - ph)
                .unwrap_or(0);
            let mut top = self.part_scroll.get(&part).copied().unwrap_or(0) as i32;
            if room > 0 {
                let step = 3 * match (&self.tui_gfx, self.tui) {
                    (Some((_, (_, ch))), true) => *ch,
                    (None, true) => 1,
                    _ => self.fonts.cell_h as i32,
                };
                top = (top - lines * step).clamp(0, room);
                self.part_scroll.insert(part.clone(), top as u32);
                self.dirty = true;
            }
            let data =
                json!({ "part": part, "lines": lines, "x": x - r.x, "y": y - r.y, "top": top });
            self.emit("part.scrolled", data);
            return;
        }
        if let Some((id, r)) = self.pane_at(x, y) {
            // the program asked for the mouse: it gets the wheel too
            let code = if lines > 0 { 64 } else { 65 };
            if !self.mods.shift_key() && self.send_mouse(id, r, x, y, code, true) {
                return;
            }
            if let Some(p) = self.panes.get(&id) {
                p.scroll(lines);
                self.dirty = true;
            }
        }
    }

    fn mouse_move(&mut self) {
        let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
        if let Some((pane, code)) = self.mouse_held {
            let drags = self.panes.get(&pane).is_some_and(|p| {
                p.term
                    .lock()
                    .mode()
                    .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
            });
            if drags {
                if let Some(r) = self.pane_rect(pane) {
                    self.send_mouse(pane, r, x, y, code + 32, true);
                }
            }
            return;
        }
        match &self.drag {
            Some(Drag::Divider {
                tab,
                path,
                side_by_side,
                area,
            }) => {
                let ratio = if *side_by_side {
                    (x - area.x) as f32 / area.w.max(1) as f32
                } else {
                    (y - area.y) as f32 / area.h.max(1) as f32
                };
                let (tab, path) = (*tab, path.clone());
                if let Some(t) = self.tabs.get_mut(tab) {
                    t.layout.set_ratio(&path, ratio);
                }
                self.resize_all();
            }
            Some(Drag::Select { pane, origin }) => {
                let (pane, origin) = (*pane, *origin);
                if let Some((pt, side)) = self.cell_point(pane, origin, x, y) {
                    if let Some(p) = self.panes.get(&pane) {
                        if let Some(s) = p.term.lock().selection.as_mut() {
                            s.update(pt, side);
                        }
                    }
                    self.dirty = true;
                }
            }
            // a floating pane follows the mouse (kept inside the area)
            Some(Drag::Float { pane, dx, dy }) => {
                let (pane, dx, dy) = (*pane, *dx, *dy);
                let area = self.content();
                if let Some(f) = self.tabs.get_mut(self.active).and_then(|t| t.float(pane)) {
                    // (min then max: never a panic, whatever its size)
                    f.x = ((x - dx - area.x) as f32 / area.w.max(1) as f32)
                        .min(1.0 - f.w)
                        .max(0.0);
                    f.y = ((y - dy - area.y) as f32 / area.h.max(1) as f32)
                        .min(1.0 - f.h)
                        .max(0.0);
                    self.dirty = true;
                }
            }
            Some(Drag::FloatSize { pane }) => {
                let pane = *pane;
                let area = self.content();
                let frame = self
                    .tabs
                    .get(self.active)
                    .and_then(|t| t.floats.iter().find(|f| f.id == pane))
                    .map(|f| self.float_frame(f));
                if let (Some(frame), Some(f)) = (
                    frame,
                    self.tabs.get_mut(self.active).and_then(|t| t.float(pane)),
                ) {
                    // from where it shows now (it may have been pushed in from the edge)
                    let (aw, ah) = (area.w.max(1) as f32, area.h.max(1) as f32);
                    f.x = (frame.x - area.x) as f32 / aw;
                    f.y = (frame.y - area.y) as f32 / ah;
                    f.w = ((x - frame.x + 1) as f32 / aw).min(1.0 - f.x).max(0.1);
                    f.h = ((y - frame.y + 1) as f32 / ah).min(1.0 - f.y).max(0.1);
                    self.resize_all();
                }
            }
            None => {}
        }
    }

    fn mouse_up(&mut self) {
        if let Some((pane, code)) = self.mouse_held.take() {
            let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
            if let Some(r) = self.pane_rect(pane) {
                self.send_mouse(pane, r, x, y, code, false);
            }
            return;
        }
        match self.drag.take() {
            Some(Drag::Select { pane, .. }) => {
                let text = self
                    .panes
                    .get(&pane)
                    .and_then(|p| p.term.lock().selection_to_string())
                    .filter(|t| !t.is_empty());
                match text {
                    Some(t) => {
                        if let Some(c) = self.clipboard.as_mut() {
                            let _ = c.set_text(t);
                        }
                    }
                    None => {
                        if let Some(p) = self.panes.get(&pane) {
                            p.term.lock().selection = None;
                        }
                    }
                }
                self.dirty = true;
            }
            Some(Drag::Divider { .. } | Drag::Float { .. } | Drag::FloatSize { .. }) | None => {}
        }
    }

    // ------------------------------------------------------------ API (`lightsout <command>`)

    fn api(&mut self, req: api::Request) {
        let a = req.args.clone();
        let pane_arg = |a: &Value| -> Option<u64> {
            a["pane"]
                .as_u64()
                .or_else(|| a["pane"].as_str().and_then(|s| s.parse().ok()))
        };
        if matches!(req.cmd.as_str(), "do" | "commands") {
            self.fresh_plugins();
        }
        let resp = match req.cmd.as_str() {
            // any command, as its key or the menu runs it (`lightsout do room.rename`)
            "do" => match a["command"].as_str().and_then(|c| self.parse_command(c)) {
                Some(act) if self.usable(&act) => {
                    let ran = act.id();
                    self.run(act);
                    json!({ "ok": true, "ran": ran })
                }
                Some(_) => json!({ "error": "that command can't run here" }),
                None => {
                    let line = a["command"].as_str().unwrap_or("").trim();
                    let id = line.split_whitespace().next().unwrap_or("");
                    match commands::COMMANDS.iter().find(|c| c.id == id) {
                        Some(c) => {
                            json!({ "error": format!("`{id}` needs a {} after it (lightsout commands)", c.arg) })
                        }
                        None => {
                            json!({ "error": format!("unknown command `{line}` (all of them: lightsout commands)") })
                        }
                    }
                }
            },
            "commands" => {
                let keys = self.keymap();
                // the built-in ones, then each plugin's (`PLUGIN.COMMAND`)
                let all: Vec<(String, String, String)> = commands::COMMANDS
                    .iter()
                    .map(|c| (c.id.to_string(), c.arg.to_string(), c.title.to_string()))
                    .chain(self.room_plugins().flat_map(|p| {
                        p.m.commands
                            .iter()
                            .filter(|c| plugins::runs_here(&c.platforms))
                            .map(|c| {
                                let title = format!("{} ({})", c.title, p.m.name);
                                (format!("{}.{}", p.m.id, c.id), String::new(), title)
                            })
                    }))
                    .collect();
                let list: Vec<Value> = all
                    .iter()
                    .map(|(id, arg, title)| {
                        let k: Vec<String> = keys
                            .iter()
                            .filter(|(_, cmd)| cmd.split(' ').next() == Some(id.as_str()))
                            .map(|(k, cmd)| match cmd.split_once(' ') {
                                Some((_, arg)) => format!("{} ({arg})", commands::key_label(k)),
                                None => commands::key_label(k),
                            })
                            .collect();
                        json!({ "id": id, "arg": arg, "title": title, "keys": k })
                    })
                    .collect();
                json!({ "commands": list })
            }
            // where every part is (the sidebar, the tab bar, plugins' parts, the panes), in
            // cells, and how many rows each plugin part shows
            "parts" => {
                let lay = self.screen_layout();
                let (cw, ch) = if self.tui {
                    (1, 1)
                } else {
                    (
                        self.fonts.cell_w.max(1) as i32,
                        self.fonts.cell_h.max(1) as i32,
                    )
                };
                let cells = |r: Rect| json!({ "x": r.x / cw, "y": r.y / ch, "cols": r.w / cw, "rows": r.h / ch });
                let mut v = Vec::new();
                if let Some(r) = lay.sidebar {
                    v.push(json!({ "id": "sidebar", "side": if lay.sidebar_right { "right" } else { "left" }, "at": cells(r) }));
                }
                if let Some(r) = lay.bar {
                    v.push(json!({ "id": "tabs", "side": if lay.bar_bottom { "bottom" } else { "top" }, "at": cells(r) }));
                }
                for (id, r, side) in &lay.parts {
                    let rows = self.part_content.get(id).map(|c| c.rows.len()).unwrap_or(0);
                    let picture = self.part_images.get(id).map(|i| json!([i.w, i.h]));
                    v.push(json!({ "id": id, "side": side, "at": cells(*r), "showing": rows, "picture": picture }));
                }
                json!({ "parts": v, "panes": cells(lay.content) })
            }
            // a plugin's part: what it shows now (`lightsout part set`), or nothing
            // a picture a plugin drew for its part (a frame)
            "part-image" => {
                let id = a["id"].as_str().unwrap_or("").to_string();
                let known = self.plugins.iter().any(|p| {
                    p.m.parts
                        .iter()
                        .any(|x| format!("{}.{}", p.m.id, x.id) == id)
                });
                let file = PathBuf::from(a["file"].as_str().unwrap_or(""));
                if !known {
                    json!({ "error": format!("no part `{id}` (a plugin's [[parts]], named PLUGIN.PART)") })
                } else {
                    match parts::load_image(&file) {
                        Ok(img) => {
                            let shown = self.screen_layout().parts.len();
                            // a new picture starts at its top, unless it is as tall as before
                            // (the same page drawn again keeps its place)
                            if self.part_images.get(&id).map(|i| i.h) != Some(img.h) {
                                self.part_scroll.remove(&id);
                            }
                            self.part_images.insert(id, img);
                            if self.screen_layout().parts.len() != shown {
                                self.resize_all();
                            }
                            json!({ "ok": true })
                        }
                        Err(e) => json!({ "error": e }),
                    }
                }
            }
            "part-set" | "part-clear" => {
                let id = a["id"].as_str().unwrap_or("").to_string();
                let known = self.plugins.iter().any(|p| {
                    p.m.parts
                        .iter()
                        .any(|x| format!("{}.{}", p.m.id, x.id) == id)
                });
                if !known {
                    json!({ "error": format!("no part `{id}` (a plugin's [[parts]], named PLUGIN.PART)") })
                } else {
                    let shown = self.screen_layout().parts.len();
                    if req.cmd == "part-set" {
                        self.part_images.remove(&id);
                        self.part_content.insert(id, parts::parse(&a["content"]));
                    } else {
                        self.part_images.remove(&id);
                        self.part_content.remove(&id);
                    }
                    // a part shown only when filled came or went: the panes make room
                    if self.screen_layout().parts.len() != shown {
                        self.resize_all();
                    }
                    json!({ "ok": true })
                }
            }
            // `lightsout plugin ...` changed something: read the plugins again
            "plugins-reload" => {
                self.load_plugins();
                json!({ "ok": true, "plugins": self.plugins.iter().map(|p| p.m.id.clone()).collect::<Vec<_>>() })
            }
            "server-rooms" => {
                self.sync_server_rooms();
                json!({ "ok": true })
            }
            "rooms" => {
                let rooms: Vec<Value> = self
                    .rooms
                    .iter()
                    .map(|r| json!({ "name": r.name, "server": r.remote.as_ref().map(|x| x.ssh.clone()) }))
                    .collect();
                json!({ "rooms": rooms })
            }
            "room-add" => {
                self.new_room(req.args["name"].as_str());
                json!({ "ok": true })
            }
            "room-remove" => match self
                .rooms
                .iter()
                .position(|r| Some(r.name.as_str()) == req.args["name"].as_str())
            {
                Some(i) => {
                    self.switch_room(i);
                    self.close_room();
                    json!({ "ok": true })
                }
                None => json!({ "ok": false }),
            },
            "show" => {
                if let Some(w) = &self.window {
                    w.set_visible(true);
                    w.focus_window();
                }
                self.hidden = false;
                json!({ "ok": true, "terminal": self.tui })
            }
            "list" => {
                let mut v = Vec::new();
                for (ri, room) in self.rooms.iter().enumerate() {
                    let tabs = if ri == self.room {
                        &self.tabs
                    } else {
                        &room.tabs
                    };
                    for (ti, t) in tabs.iter().enumerate() {
                        for id in t.panes() {
                            if let Some(p) = self.panes.get(&id) {
                                // a floating one: where it is, in % of the area (for window managers)
                                let float = t.floats.iter().find(|f| f.id == id).map(|f| json!({ "x": (f.x * 100.0).round(), "y": (f.y * 100.0).round(), "w": (f.w * 100.0).round(), "h": (f.h * 100.0).round(), "minimized": f.min }));
                                v.push(json!({ "id": id, "room": room.name, "tab": ti + 1, "title": p.title, "cwd": p.cwd.to_string_lossy(), "state": p.det.state.name(), "command": p.command, "running": p.running, "float": float }));
                            }
                        }
                    }
                }
                json!({ "panes": v })
            }
            "new" => {
                let cwd = a["cwd"]
                    .as_str()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.cwd());
                let cmd = a["command"]
                    .as_str()
                    .map(str::to_string)
                    .filter(|c| !c.is_empty());
                let id = match a["split"].as_str() {
                    Some("right") => self.split(true, cmd, pane_arg(&a), Vec::new()),
                    Some("down") => self.split(false, cmd, pane_arg(&a), Vec::new()),
                    _ => self.new_tab(cwd, cmd, Vec::new()),
                };
                match id {
                    Some(id) => json!({ "id": id }),
                    None => json!({ "error": "could not open a pane" }),
                }
            }
            "send" => match pane_arg(&a).and_then(|id| self.panes.get(&id)) {
                Some(p) => {
                    let mut t = a["text"].as_str().unwrap_or("").to_string();
                    if a["enter"].as_bool().unwrap_or(true) {
                        t.push('\r');
                    }
                    p.input(t.into_bytes());
                    json!({ "ok": true })
                }
                None => json!({ "error": "no such pane" }),
            },
            "read" => match pane_arg(&a).and_then(|id| self.panes.get(&id)) {
                Some(p) => {
                    json!({ "text": p.text(a["lines"].as_u64().unwrap_or(60) as usize), "state": p.det.state.name() })
                }
                None => json!({ "error": "no such pane" }),
            },
            "explain" => match pane_arg(&a).and_then(|id| self.panes.get(&id)) {
                Some(p) => json!({ "state": p.det.state.name(), "reason": p.det.reason }),
                None => json!({ "error": "no such pane" }),
            },
            "wait" => match pane_arg(&a) {
                Some(id) if self.panes.contains_key(&id) => {
                    let until: Vec<State> = match a["until"].as_str() {
                        Some("idle") => vec![State::Idle],
                        Some("done") => vec![State::Done],
                        Some("needs-you") => vec![State::NeedsYou],
                        _ => vec![State::Done, State::Idle, State::NeedsYou],
                    };
                    let deadline =
                        Instant::now() + Duration::from_secs(a["timeout"].as_u64().unwrap_or(600));
                    // give the agent a moment to start before checking
                    if let Some(p) = self.panes.get_mut(&id) {
                        p.det.on_output();
                    }
                    self.waiters.push(Waiter {
                        pane: id,
                        until,
                        deadline,
                        reply: req.reply,
                        since: Instant::now(),
                        seen_busy: false,
                    });
                    return;
                }
                _ => json!({ "error": "no such pane" }),
            },
            "focus" => match pane_arg(&a) {
                Some(id) if self.panes.contains_key(&id) => {
                    self.focus(id);
                    json!({ "ok": true })
                }
                _ => json!({ "error": "no such pane" }),
            },
            // the same as "Close pane" in the actions: its program is stopped
            "close" => match pane_arg(&a) {
                Some(id) if self.panes.contains_key(&id) => {
                    self.close_pane(id);
                    json!({ "ok": true })
                }
                _ => json!({ "error": "no such pane" }),
            },
            "notify" => {
                let t = a["text"].as_str().unwrap_or("").to_string();
                self.toast(t, self.theme.accent);
                json!({ "ok": true })
            }
            // an agent's hook: the conversation running in this tab (`lightsout report-session`)
            "report-session" => match pane_arg(&a).and_then(|id| self.panes.get_mut(&id)) {
                Some(p) => {
                    p.agent_session = a["id"].as_str().map(str::to_string);
                    json!({ "ok": true })
                }
                None => json!({ "error": "no such pane" }),
            },
            "screen" => {
                let on = a["on"].as_bool().unwrap_or(true);
                platform::screen(on);
                self.screen_off = !on;
                if !on {
                    self.emit("screen.off", json!({}));
                }
                json!({ "ok": true })
            }
            "power" => match platform::battery() {
                Some(b) => {
                    json!({ "percent": b.percent, "watts": b.watts, "charging": b.charging, "hours_left": b.hours_left })
                }
                None => json!({ "battery": null }),
            },
            "quit" => {
                self.quit_saved();
                json!({ "ok": true })
            }
            "switch" => {
                if platform::in_lightsout() {
                    self.lights_on();
                } else {
                    self.start_switch();
                }
                json!({ "ok": true })
            }
            _ => json!({ "error": format!("unknown command `{}`", req.cmd) }),
        };
        let _ = req.reply.send(resp);
        self.dirty = true;
    }

    // ------------------------------------------------------------ drawing

    fn refresh_battery(&mut self) {
        if self.battery_at.elapsed() < Duration::from_secs(20) {
            return;
        }
        self.battery_at = Instant::now();
        let battery = platform::battery();
        self.on_battery = battery.as_ref().is_some_and(|b| !b.charging);
        let text = match &battery {
            Some(b) => {
                self.hours_avg = b
                    .hours_left
                    .map(|h| self.hours_avg.map_or(h, |a| a * 0.7 + h * 0.3));
                let left = self.hours_avg.map(|h| format!(" {}", time_left(h)));
                let w = b
                    .watts
                    .filter(|w| *w > 0.05 && !b.charging)
                    .map(|w| format!(" {w:.1} W"))
                    .unwrap_or_default();
                let plus = if b.charging { "+" } else { "" };
                let plug = if !b.charging && b.percent <= LOW_BATTERY {
                    "plug in! "
                } else {
                    ""
                };
                format!("{plug}{plus}{}%{}{w}", b.percent, left.unwrap_or_default())
            }
            None => String::new(),
        };
        if text != self.battery_text {
            self.battery_text = text;
            self.dirty = true;
        }
        let Some(b) = battery.filter(|_| watches_battery()) else {
            return;
        };
        match low_battery(b.percent, b.charging, self.battery_warned) {
            LowBattery::Fine => self.battery_warned &= !b.charging && b.percent <= LOW_BATTERY,
            LowBattery::Warn => {
                self.battery_warned = true;
                self.emit("battery.low", json!({ "percent": b.percent }));
                self.toast(
                    format!("Battery {}%: plug in. At {EMPTY_BATTERY}% Lights Out saves your tabs and shuts down.", b.percent),
                    ALERT,
                );
            }
            LowBattery::ShutDown => {
                self.toast(
                    "Battery empty: saving your tabs and shutting down.".into(),
                    ALERT,
                );
                platform::shut_down_saved(&self.saved(80));
            }
        }
    }

    /// What the bar and sidebar show: each tab's title and state; each room and what runs in it.
    #[allow(clippy::type_complexity)]
    fn summary(&self) -> (Vec<(String, State)>, Vec<(String, Vec<AgentRow>)>) {
        let tab_states: Vec<(String, State)> = self
            .tabs
            .iter()
            .map(|tv| {
                let leaves = tv.panes();
                let title = self
                    .panes
                    .get(&tv.focus)
                    .map(|p| p.title.clone())
                    .unwrap_or_default();
                let state = leaves
                    .iter()
                    .filter_map(|id| self.panes.get(id))
                    .map(|p| p.det.state)
                    .max_by_key(|s| urgency(*s))
                    .unwrap_or(State::Idle);
                (
                    if leaves.len() > 1 {
                        format!("{title} +{}", leaves.len() - 1)
                    } else {
                        title
                    },
                    state,
                )
            })
            .collect();
        let agent_of = |id: u64| -> Option<AgentRow> {
            let p = self.panes.get(&id)?;
            let program = p.running.clone()?;
            let folder = p
                .cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // runtimes like node/python say little; their window title usually says more
            let generic = ["node", "python", "python3", "bun", "deno", "java", "ruby"]
                .contains(&program.as_str());
            let name = if generic && !p.title.is_empty() && p.title != folder {
                p.title.clone()
            } else {
                program
            };
            Some((id, name, p.det.state, folder))
        };
        let rooms: Vec<(String, Vec<AgentRow>)> = self
            .rooms
            .iter()
            .enumerate()
            .map(|(ri, r)| {
                let tabs = if ri == self.room { &self.tabs } else { &r.tabs };
                if r.remote.is_some() {
                    // what runs over there; a click goes to this room's view of it
                    let view = tabs.first().map(|t| t.focus).unwrap_or(0);
                    let state = |s: &str| match s {
                        "working" => State::Working,
                        "needs-you" => State::NeedsYou,
                        "done" => State::Done,
                        _ => State::Idle,
                    };
                    let rows = r
                        .remote_agents
                        .iter()
                        .map(|(n, s, f)| (view, n.clone(), state(s), f.clone()))
                        .collect();
                    return (r.name.clone(), rows);
                }
                (
                    r.name.clone(),
                    tabs.iter()
                        .flat_map(|tv| tv.panes())
                        .filter_map(agent_of)
                        .collect(),
                )
            })
            .collect();
        (tab_states, rooms)
    }

    fn draw(&mut self) {
        let t = self.theme;
        let (w, h) = self.size;
        let bar = self.bar_h();
        let content = self.content();
        // the tree's panes; the floating ones (frame, where their program shows) on top
        let floats: Vec<(u64, Rect, Rect)> = self
            .float_frames()
            .into_iter()
            .map(|(id, frame)| (id, frame, self.float_body(frame)))
            .collect();
        let active_rects: Vec<(u64, Rect)> = self
            .active_rects()
            .into_iter()
            .filter(|(id, _)| !floats.iter().any(|f| f.0 == *id))
            .collect();
        let dividers = self
            .tabs
            .get(self.active)
            .map(|tv| tv.layout.layout(content).1)
            .unwrap_or_default();
        let focused = self.focused();
        let (tab_states, rooms) = self.summary();
        let popup = self.popup.map(|p| (p, self.popup_rect()));
        let sel = self.sel();
        let layout = self.screen_layout();
        let palette = self.palette();
        let title_h = self.title_h();

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let mut cv = Canvas {
            buf: &mut buffer,
            w,
            h,
            ox: 0,
            oy: 0,
        };
        cv.fill(t.bg);
        let f = &mut self.fonts;
        let (cw, ch) = (f.cell_w as i32, f.cell_h as i32);
        let center = |s: &str| (w as i32 - s.chars().count() as i32 * cw) / 2;

        if let Lock::Locked { wrong, .. } = &self.lock {
            let cy = h as i32 / 2 - ch * 2;
            let s = "L I G H T S   O U T";
            cv.text(f, center(s), cy, s, true, t.fg);
            let s = if *wrong > 0 {
                "Wrong PIN. Try again."
            } else {
                "Locked. Type your PIN and press Enter."
            };
            cv.text(
                f,
                center(s),
                cy + ch * 2,
                s,
                false,
                if *wrong > 0 { t.fg } else { DIMTEXT },
            );
            let dots: String = "●".repeat(self.pin.chars().count());
            cv.text(f, center(&dots), cy + ch * 4, &dots, false, t.fg);
            present(buffer, &mut self.shown, w);
            return;
        }

        self.hits.clear();
        self.scroll_areas.clear();
        self.part_clicks.clear();
        let lo = platform::in_lightsout();
        // ---- the tab bar (top or bottom): tabs left; your items, battery and buttons right.
        // Drawn as if at the top left, then moved where it is (with its click areas).
        if let Some(br) = layout.bar {
            let (hits0, scroll0) = (self.hits.len(), self.scroll_areas.len());
            (cv.ox, cv.oy) = (br.x, br.y);
            let (sw, w, bar) = (0, br.w as u32, br.h);
            cv.rect(sw, 0, w - sw as u32, bar as u32, BAR);
            let line_y = if layout.bar_bottom { 0 } else { bar - 1 };
            cv.rect(sw, line_y, w - sw as u32, 1, BORDER);
            let ty = 5;
            let mut right: Vec<(String, Option<Hit>)> = self
                .bar_texts
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| (s.clone(), None))
                .collect();
            if layout.sidebar.is_none() {
                // no sidebar: its battery and buttons come up here
                right.push((self.battery_text.clone(), None));
                right.push((
                    if lo { "Lights On" } else { "Lights Out" }.into(),
                    Some(Hit::Switch),
                ));
                right.push(("actions".into(), Some(Hit::Palette)));
            }
            let right_w: i32 = right
                .iter()
                .filter(|(s, _)| !s.is_empty())
                .map(|(s, _)| (s.chars().count() as i32 + 1) * cw)
                .sum();
            // tabs: as many as fit; the rest scroll (wheel, or ‹ ›)
            let labels: Vec<(String, i32)> = tab_states
                .iter()
                .enumerate()
                .map(|(i, (title, _))| {
                    let title: String = title.chars().take(24).collect();
                    let label = format!("  {} {}  ", i + 1, title);
                    let wpx = (label.chars().count() as i32 + 1) * cw;
                    (label, wpx)
                })
                .collect();
            let x0 = sw + PADX;
            let limit = w as i32 - PADX - right_w - 6 * cw;
            if self.active != self.seen_tab {
                self.seen_tab = self.active;
                if self.active < self.tabs_first {
                    self.tabs_first = self.active;
                }
                while self.tabs_first < self.active {
                    let used: i32 = labels[self.tabs_first..=self.active]
                        .iter()
                        .map(|l| l.1 + 2)
                        .sum::<i32>()
                        + 2 * cw;
                    if x0 + used <= limit {
                        break;
                    }
                    self.tabs_first += 1;
                }
            }
            self.tabs_first = self.tabs_first.min(labels.len().saturating_sub(1));
            let mut x = x0;
            if self.tabs_first > 0 {
                cv.text(f, x, ty, "‹", false, t.fg);
                self.hits.push((
                    Rect {
                        x,
                        y: 0,
                        w: 2 * cw,
                        h: bar,
                    },
                    Hit::TabsLeft,
                ));
                x += 2 * cw;
            }
            let mut more_right = false;
            for (i, (label, wpx)) in labels.iter().enumerate().skip(self.tabs_first) {
                if x + wpx > limit {
                    more_right = true;
                    break;
                }
                let (mark, mark_color) = state_mark(tab_states[i].1);
                cv.ch(f, x + cw / 2, ty, mark, false, mark_color);
                cv.text(
                    f,
                    x + cw,
                    ty,
                    label,
                    i == self.active,
                    if i == self.active { t.fg } else { DIMTEXT },
                );
                self.hits.push((
                    Rect {
                        x,
                        y: 0,
                        w: *wpx,
                        h: bar,
                    },
                    Hit::Tab(i),
                ));
                x += wpx + 2;
            }
            if more_right {
                cv.text(f, x, ty, "›", false, t.fg);
                self.hits.push((
                    Rect {
                        x,
                        y: 0,
                        w: 2 * cw,
                        h: bar,
                    },
                    Hit::TabsRight,
                ));
                x += 2 * cw;
            }
            cv.text(f, x, ty, " + ", false, DIMTEXT);
            self.hits.push((
                Rect {
                    x,
                    y: 0,
                    w: 3 * cw,
                    h: bar,
                },
                Hit::NewTab,
            ));
            x += 3 * cw;
            self.scroll_areas.push((
                Rect {
                    x: sw,
                    y: 0,
                    w: w as i32 - sw,
                    h: bar,
                },
                Scroll::Tabs,
            ));
            let mut rx = w as i32 - PADX;
            for (s, hit) in right.into_iter().rev() {
                if s.is_empty() {
                    continue;
                }
                let wpx = s.chars().count() as i32 * cw;
                rx -= wpx;
                if rx < x + 4 * cw {
                    break;
                }
                cv.text(
                    f,
                    rx,
                    ty,
                    &s,
                    false,
                    if hit.is_some() { t.fg } else { DIMTEXT },
                );
                if let Some(hh) = hit {
                    self.hits.push((
                        Rect {
                            x: rx,
                            y: 0,
                            w: wpx,
                            h: bar,
                        },
                        hh,
                    ));
                }
                rx -= cw;
            }

            (cv.ox, cv.oy) = (0, 0);
            for hit in &mut self.hits[hits0..] {
                (hit.0.x, hit.0.y) = (hit.0.x + br.x, hit.0.y + br.y);
            }
            for a in &mut self.scroll_areas[scroll0..] {
                (a.0.x, a.0.y) = (a.0.x + br.x, a.0.y + br.y);
            }
        }

        // ---- sidebar (left or right): top half = rooms; bottom half = what runs in this room,
        // one line each. Drawn as if at the top left, then moved where it is.
        if let Some(sr) = layout.sidebar {
            let (hits0, scroll0) = (self.hits.len(), self.scroll_areas.len());
            (cv.ox, cv.oy) = (sr.x, sr.y);
            let (sw, h) = (sr.w, sr.h as u32);
            let side = Rect {
                x: 0,
                y: 0,
                w: sw,
                h: h as i32,
            };
            // its line on the side of the panes
            let line_x = if layout.sidebar_right { 0 } else { side.w - 1 };
            cv.rect(line_x, side.y, 1, side.h as u32, BORDER);
            let max_chars = (sw / cw - 3).max(4) as usize;
            let bottom = side.h - 3 * ch - 12;
            let mid = bottom / 2;
            let row = ch + 6;
            // rooms: a small heading, a bit of space, then the names (they scroll)
            cv.text(f, PADX, 10, "rooms", false, 0x55555c);
            let y0 = 10 + ch + 10;
            let fit = ((mid - y0) / row).max(1) as usize;
            let total = rooms.len() + 1; // + room
            if self.room != self.seen_room {
                self.seen_room = self.room;
                if self.room < self.rooms_top {
                    self.rooms_top = self.room;
                } else if self.room >= self.rooms_top + fit {
                    self.rooms_top = self.room + 1 - fit;
                }
            }
            self.rooms_top = self.rooms_top.min(total.saturating_sub(fit));
            let mut y = y0;
            for i in self.rooms_top..total.min(self.rooms_top + fit) {
                if let Some((name, _)) = rooms.get(i) {
                    let current = i == self.room;
                    let label: String = name.chars().take(max_chars).collect();
                    let server = self
                        .rooms
                        .get(i)
                        .and_then(|r| r.remote.as_ref().map(|x| (x.server.clone(), r.offline)));
                    // a server that can't be reached right now: its room dims
                    let off = server.as_ref().is_some_and(|m| m.1);
                    cv.text(
                        f,
                        PADX,
                        y,
                        &label,
                        current,
                        if current {
                            t.fg
                        } else if off {
                            0x3a3a42
                        } else {
                            DIMTEXT
                        },
                    );
                    // renamed: the server room's own name, dimmed, after it
                    if let Some((m, _)) = server.filter(|m| &m.0 != name) {
                        let mx = sw - PADX - (m.chars().count() as i32 + 1) * cw;
                        cv.text(f, mx, y, &m, false, 0x55555c);
                    }
                    self.hits.push((
                        Rect {
                            x: 0,
                            y: y - 3,
                            w: sw,
                            h: row,
                        },
                        Hit::Room(i),
                    ));
                } else {
                    cv.text(f, PADX, y, "+ room", false, 0x55555c);
                    self.hits.push((
                        Rect {
                            x: 0,
                            y: y - 3,
                            w: sw,
                            h: row,
                        },
                        Hit::NewRoom,
                    ));
                }
                y += row;
            }
            scroll_marks(
                &mut cv,
                f,
                sw - PADX - 2 * cw,
                10,
                self.rooms_top > 0,
                self.rooms_top + fit < total,
            );
            self.scroll_areas.push((
                Rect {
                    x: 0,
                    y: 0,
                    w: sw,
                    h: mid,
                },
                Scroll::Rooms,
            ));
            cv.rect(PADX, mid, (sw - 2 * PADX) as u32, 1, BORDER);
            // what runs in this room (agents and anything else): heading, space, one line each
            cv.text(f, PADX, mid + 12, "running", false, 0x55555c);
            let y0 = mid + 12 + ch + 10;
            let mut y = y0;
            if let Some((_, agents)) = rooms.get(self.room) {
                let fit = ((bottom - y0) / row).max(1) as usize;
                self.running_top = self.running_top.min(agents.len().saturating_sub(fit));
                scroll_marks(
                    &mut cv,
                    f,
                    sw - PADX - 2 * cw,
                    mid + 12,
                    self.running_top > 0,
                    self.running_top + fit < agents.len(),
                );
                self.scroll_areas.push((
                    Rect {
                        x: 0,
                        y: mid,
                        w: sw,
                        h: bottom - mid,
                    },
                    Scroll::Running,
                ));
                for (id, title, state, folder) in agents.iter().skip(self.running_top).take(fit) {
                    let (mark, mark_color) = state_mark(*state);
                    cv.ch(f, PADX, y, mark, false, mark_color);
                    let name: String = title.chars().take(max_chars - 2).collect();
                    cv.text(f, PADX + 2 * cw, y, &name, false, 0xc8c8d0);
                    let used = name.chars().count() + 3;
                    if !folder.is_empty() && folder != title && used + 2 < max_chars {
                        let rest: String = folder.chars().take(max_chars - used).collect();
                        cv.text(f, PADX + (used as i32 + 1) * cw, y, &rest, false, DIMTEXT);
                    }
                    self.hits.push((
                        Rect {
                            x: 0,
                            y: y - 3,
                            w: sw,
                            h: row,
                        },
                        Hit::Pane(*id),
                    ));
                    y += row;
                }
            }
            // bottom left
            let by = side.h - 2 * ch - 10;
            cv.text(f, PADX, by, &self.battery_text, false, DIMTEXT);
            // Windows: "Lights Out" switches over; in Lights Out: "Lights On" goes back to Windows
            let label = if lo { "Lights On" } else { "Lights Out" };
            cv.text(f, PADX, by + ch + 2, label, false, t.fg);
            self.hits.push((
                Rect {
                    x: PADX,
                    y: by + ch,
                    w: label.len() as i32 * cw,
                    h: ch + 4,
                },
                Hit::Switch,
            ));
            let ax = PADX + (label.len() as i32 + 3) * cw;
            cv.text(f, ax, by + ch + 2, "actions", false, t.fg);
            self.hits.push((
                Rect {
                    x: ax,
                    y: by + ch,
                    w: 7 * cw,
                    h: ch + 4,
                },
                Hit::Palette,
            ));
            (cv.ox, cv.oy) = (0, 0);
            for hit in &mut self.hits[hits0..] {
                (hit.0.x, hit.0.y) = (hit.0.x + sr.x, hit.0.y + sr.y);
            }
            for a in &mut self.scroll_areas[scroll0..] {
                (a.0.x, a.0.y) = (a.0.x + sr.x, a.0.y + sr.y);
            }
        }

        // ---- plugins' parts: their rows of pieces; a piece with a command can be clicked
        for (id, r, side) in &layout.parts {
            let content = self.part_content.get(id);
            let pal = palette;
            let bg = parts::color(content.and_then(|c| c.bg.as_deref()), pal, BAR);
            cv.rect(r.x, r.y, r.w as u32, r.h as u32, bg);
            // a line on the side of the panes
            match side.as_str() {
                "left" => cv.rect(r.x + r.w - 1, r.y, 1, r.h as u32, BORDER),
                "right" => cv.rect(r.x, r.y, 1, r.h as u32, BORDER),
                "top" => cv.rect(r.x, r.y + r.h - 1, r.w as u32, 1, BORDER),
                _ => cv.rect(r.x, r.y, r.w as u32, 1, BORDER),
            }
            // a picture the plugin drew (a frame): as it is, from the top left
            if let Some(img) = self.part_images.get(id) {
                let top = self.part_scroll.get(id).copied().unwrap_or(0);
                cv.image(r.x, r.y, r.w, r.h, img.w, &img.px, top);
                continue;
            }
            let Some(content) = content else {
                continue;
            };
            let max_x = r.x + r.w - PADX;
            for (n, row) in content.rows.iter().enumerate() {
                let y = r.y + 4 + n as i32 * ch;
                if y + ch > r.y + r.h {
                    break;
                }
                let mut x = r.x + PADX;
                for sp in row {
                    let room_left = ((max_x - x) / cw).max(0) as usize;
                    let text: String = sp.text.chars().take(room_left).collect();
                    if text.is_empty() {
                        continue;
                    }
                    let color = parts::color(sp.color.as_deref(), pal, t.fg);
                    let end = cv.text(f, x, y, &text, sp.bold, color);
                    if let Some(cmd) = &sp.command {
                        self.hits.push((
                            Rect {
                                x,
                                y,
                                w: end - x,
                                h: ch,
                            },
                            Hit::Part(self.part_clicks.len()),
                        ));
                        self.part_clicks.push(cmd.clone());
                    }
                    x = end;
                }
            }
        }

        // ---- panes
        let draw_pane = |cv: &mut Canvas, f: &mut Fonts, p: &Pane, r: Rect| {
            let term = p.term.lock();
            let content = term.renderable_content();
            let colors = content.colors;
            let offset = content.display_offset as i32;
            let selection = content.selection;
            let top = r.y + 2;
            for cell in content.display_iter {
                let row = cell.point.line.0 + offset;
                if row < 0 || row >= p.size.rows as i32 || cell.point.column.0 >= p.size.cols {
                    continue;
                }
                let flags = cell.flags;
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let mut fg = resolve(cell.fg, colors, t);
                let mut bg = resolve(cell.bg, colors, t);
                if flags.contains(Flags::DIM) {
                    fg = dim(fg);
                }
                let selected = selection.is_some_and(|s| s.contains(cell.point));
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if selected {
                    bg = 0x3a4a6a;
                }
                if flags.contains(Flags::HIDDEN) {
                    fg = bg;
                }
                let px = r.x + PADX + cell.point.column.0 as i32 * cw;
                let py = top + row * ch;
                let width = if flags.contains(Flags::WIDE_CHAR) {
                    2
                } else {
                    1
                };
                if bg != t.bg {
                    cv.rect(px, py, (cw * width) as u32, ch as u32, bg);
                }
                cv.ch(f, px, py, cell.c, flags.contains(Flags::BOLD), fg);
                if flags.intersects(Flags::ALL_UNDERLINES) {
                    cv.rect(px, py + ch - 2, (cw * width) as u32, 1, fg);
                }
            }
            let cur = content.cursor;
            if content.mode.contains(TermMode::SHOW_CURSOR)
                && cur.shape != CursorShape::Hidden
                && focused == Some(p.id)
            {
                let row = cur.point.line.0 + offset;
                let px = r.x + PADX + cur.point.column.0 as i32 * cw;
                let py = top + row * ch;
                match cur.shape {
                    CursorShape::Beam => cv.rect(px, py, 2, ch as u32, t.fg),
                    CursorShape::Underline => cv.rect(px, py + ch - 2, cw as u32, 2, t.fg),
                    _ => {
                        cv.rect(px, py, cw as u32, ch as u32, 0x9a9aa5);
                        let c = term.grid()[cur.point].c;
                        cv.ch(f, px, py, c, false, t.bg);
                    }
                }
            }
        };
        let multi = active_rects.len() > 1;
        for (id, r) in &active_rects {
            if let Some(p) = self.panes.get(id) {
                draw_pane(&mut cv, f, p, *r);
                if multi && focused != Some(*id) {
                    let (mark, mark_color) = state_mark(p.det.state);
                    cv.ch(f, r.x + r.w - cw - 2, r.y + 2, mark, false, mark_color);
                }
            }
        }
        for d in &dividers {
            if d.side_by_side {
                cv.rect(d.at, d.area.y, layout::GAP as u32, d.area.h as u32, BORDER);
            } else {
                cv.rect(d.area.x, d.at, d.area.w as u32, layout::GAP as u32, BORDER);
            }
        }
        // floating panes, bottom to top: a thin border, a title bar (state, title; minimise,
        // back into the layout, close), the program; the bottom right corner resizes
        for (id, frame, body) in &floats {
            let Some(p) = self.panes.get(id) else {
                continue;
            };
            let on = focused == Some(*id);
            cv.rect(frame.x, frame.y, frame.w as u32, frame.h as u32, BORDER);
            let bar_bg = if on { 0x26262c } else { 0x141418 };
            cv.rect(
                frame.x + 1,
                frame.y + 1,
                (frame.w - 2).max(0) as u32,
                (title_h - 1).max(0) as u32,
                bar_bg,
            );
            cv.rect(body.x, body.y, body.w as u32, body.h as u32, t.bg);
            let ty = frame.y + (title_h - ch) / 2;
            let (mark, mark_color) = state_mark(p.det.state);
            cv.ch(f, frame.x + cw / 2 + 2, ty, mark, false, mark_color);
            let buttons = [
                ("–", Hit::FloatMin(*id)),
                ("▢", Hit::FloatTile(*id)),
                ("×", Hit::FloatClose(*id)),
            ];
            let bx0 = frame.x + frame.w - (buttons.len() as i32 * 3) * cw;
            let room_for_title = ((bx0 - frame.x) / cw - 4).max(0) as usize;
            let title: String = p.title.chars().take(room_for_title).collect();
            cv.text(
                f,
                frame.x + 2 * cw,
                ty,
                &title,
                on,
                if on { t.fg } else { DIMTEXT },
            );
            self.hits.push((
                Rect {
                    h: title_h,
                    ..*frame
                },
                Hit::FloatTitle(*id),
            ));
            for (n, (label, hit)) in buttons.into_iter().enumerate() {
                let bx = bx0 + n as i32 * 3 * cw;
                cv.text(
                    f,
                    bx + cw,
                    ty,
                    label,
                    false,
                    if on { t.fg } else { DIMTEXT },
                );
                self.hits.push((
                    Rect {
                        x: bx,
                        y: frame.y,
                        w: 3 * cw,
                        h: title_h,
                    },
                    hit,
                ));
            }
            draw_pane(&mut cv, f, p, *body);
            self.hits.push((
                Rect {
                    x: frame.x + frame.w - cw,
                    y: frame.y + frame.h - ch,
                    w: cw,
                    h: ch,
                },
                Hit::FloatCorner(*id),
            ));
        }
        if let Some((id, r)) = popup {
            cv.rect(r.x - 1, r.y - 1, (r.w + 2) as u32, (r.h + 2) as u32, BORDER);
            cv.rect(r.x, r.y, r.w as u32, r.h as u32, t.bg);
            if let Some(p) = self.panes.get(&id) {
                draw_pane(&mut cv, f, p, r);
            }
        }

        // ---- overlays
        match &self.overlay {
            Overlay::Palette {
                title,
                items,
                filter,
                picked,
                top,
                ..
            } => {
                let f_l = filter.to_lowercase();
                let shown: Vec<&Item> = items
                    .iter()
                    .filter(|i| f_l.is_empty() || i.label.to_lowercase().contains(&f_l))
                    .collect();
                let pw = (w as i32 * 6 / 10).max(40 * cw).min(w as i32 - 20);
                let px = (w as i32 - pw) / 2;
                let rows = shown.len().min(16) as i32;
                let ph = (rows + 3) * ch + 12;
                let py = ((h as i32 - ph) / 2).max(bar + 4);
                cv.rect(px - 1, py - 1, (pw + 2) as u32, (ph + 2) as u32, BORDER);
                cv.rect(px, py, pw as u32, ph as u32, 0x000000);
                cv.text(f, px + cw, py + 6, title, true, t.fg);
                cv.text(f, px + cw, py + 6 + ch, &format!("› {filter}"), false, t.fg);
                self.palette_rows = rows.max(1) as usize;
                let start = (*top).min(shown.len().saturating_sub(rows as usize));
                for (n, it) in shown.iter().enumerate().skip(start).take(rows as usize) {
                    let y = py + 6 + (2 + (n - start) as i32) * ch;
                    let on = *picked && n == sel;
                    self.hits.push((
                        Rect {
                            x: px,
                            y,
                            w: pw,
                            h: ch,
                        },
                        Hit::PaletteItem(n),
                    ));
                    cv.text(f, px + cw, y, if on { "›" } else { " " }, true, t.fg);
                    cv.text(
                        f,
                        px + 3 * cw,
                        y,
                        &it.label,
                        on,
                        if on { t.fg } else { DIMTEXT },
                    );
                    let hx = px + pw - (it.hint.chars().count() as i32 + 1) * cw;
                    cv.text(f, hx, y, &it.hint, false, DIMTEXT);
                }
                scroll_marks(
                    &mut cv,
                    f,
                    px + pw - 3 * cw,
                    py + 6,
                    start > 0,
                    start + (rows as usize) < shown.len(),
                );
                self.hits.push((
                    Rect {
                        x: px,
                        y: py,
                        w: pw,
                        h: ph,
                    },
                    Hit::PaletteBox,
                ));
            }
            Overlay::Menu {
                x: mx,
                y: my,
                items,
                sel,
            } => {
                let mw = items
                    .iter()
                    .map(|i| i.label.chars().count() + i.hint.chars().count() + 4)
                    .max()
                    .unwrap_or(10) as i32
                    * cw;
                let mh = items.len() as i32 * ch + 8;
                let (mx, my) = ((*mx).min(w as i32 - mw - 2), (*my).min(h as i32 - mh - 2));
                cv.rect(mx - 1, my - 1, (mw + 2) as u32, (mh + 2) as u32, BORDER);
                cv.rect(mx, my, mw as u32, mh as u32, 0x000000);
                for (n, it) in items.iter().enumerate() {
                    let y = my + 4 + n as i32 * ch;
                    if n == *sel {
                        cv.rect(mx + 2, y, (mw - 4) as u32, ch as u32, BAR_ACTIVE);
                    }
                    cv.text(
                        f,
                        mx + cw,
                        y,
                        &it.label,
                        n == *sel,
                        if n == *sel { t.fg } else { 0xa8a8b0 },
                    );
                    cv.text(
                        f,
                        mx + mw - (it.hint.chars().count() as i32 + 1) * cw,
                        y,
                        &it.hint,
                        false,
                        DIMTEXT,
                    );
                }
            }
            Overlay::Confirm(c) => {
                let s = match c {
                    Confirm::BackToWindows if platform::image() => "Lights On? Windows starts again; your tabs come along. Enter = yes, Esc = no",
                    Confirm::BackToWindows => "Lights On? The desktop comes back; your tabs come along. Enter = yes, Esc = no",
                    Confirm::EnterLightsOut if cfg!(windows) => "Lights Out? Windows shuts down, your tabs reopen there. Enter = yes, Esc = no",
                    Confirm::EnterLightsOut if cfg!(target_os = "macos") => "Lights Out? Other apps quit (unsaved ones ask first), the Mac goes quiet. Enter = yes, Esc = no",
                    Confirm::EnterLightsOut => "Lights Out? The desktop and its apps close (save your work first). Enter = yes, Esc = no",
                    Confirm::Quit => "Quit Lights Out? All agents stop (tabs come back next time). Enter = yes, Esc = no",
                };
                let y = h as i32 - ch - 8;
                cv.rect(0, y - 4, w, (ch + 12) as u32, BAR_ACTIVE);
                cv.text(f, PADX * 2, y, s, true, t.fg);
            }
            Overlay::Ask {
                title,
                text,
                purpose,
            } => {
                // passwords show as dots
                let text = if matches!(purpose, Purpose::WifiPassword(_)) {
                    "•".repeat(text.chars().count())
                } else {
                    text.clone()
                };
                let y = h as i32 - 2 * ch - 12;
                cv.rect(0, y - 4, w, (2 * ch + 16) as u32, BAR_ACTIVE);
                cv.text(f, PADX * 2, y, title, true, t.fg);
                cv.text(f, PADX * 2, y + ch + 2, &format!("› {text}"), false, t.fg);
            }
            Overlay::None => {}
        }

        // ---- popups (toasts), bottom right
        let mut toast_y = h as i32 - 12;
        for toast in self.toasts.iter().rev() {
            let tw = (toast.text.chars().count() as i32 + 2) * cw;
            toast_y -= ch + 10;
            let tx = w as i32 - tw - 12;
            let _ = toast.color;
            cv.rect(
                tx - 1,
                toast_y - 1,
                (tw + 2) as u32,
                (ch + 8) as u32,
                BORDER,
            );
            cv.rect(tx, toast_y, tw as u32, (ch + 6) as u32, 0x000000);
            cv.text(f, tx + cw, toast_y + 3, &toast.text, false, t.fg);
        }
        present(buffer, &mut self.shown, w);
    }

    /// The settings and the plugins, read again when their files changed.
    fn reload_config_if_changed(&mut self) {
        self.cfg_checked = Instant::now();
        self.fresh_plugins();
        if !config::changed_since(self.cfg_modified) {
            return;
        }
        let loaded = config::load();
        self.cfg_modified = loaded.modified;
        match loaded.error {
            Some(e) => self.toast(e, ALERT),
            None => {
                let font_before = self.cfg.theme.font_size;
                self.cfg = loaded.config;
                self.theme = theme_from(&self.cfg);
                // a key you just set may take one a plugin had
                self.warn_skipped_keys();
                if !self.tui
                    && self.cfg.theme.font_size > 0.0
                    && self.cfg.theme.font_size != font_before
                {
                    let scale = self
                        .window
                        .as_ref()
                        .map(|w| w.scale_factor() as f32)
                        .unwrap_or(1.0);
                    self.set_font(self.cfg.theme.font_size * scale);
                }
                self.toast("Settings applied".into(), DIMTEXT);
            }
        }
        self.dirty = true;
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Lights Out")
            .with_inner_size(LogicalSize::new(1180, 740));
        if platform::in_lightsout() {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Rc::new(el.create_window(attrs).expect("window"));
        let context = softbuffer::Context::new(window.clone()).expect("display");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
        let size = window.inner_size();
        self.size = (size.width.max(1), size.height.max(1));
        let base = if self.cfg.theme.font_size > 0.0 {
            self.cfg.theme.font_size
        } else {
            15.0
        };
        let mut px = base * window.scale_factor() as f32;
        if platform::in_lightsout() && self.cfg.theme.font_size <= 0.0 {
            px = px.max(self.size.1 as f32 / 58.0); // readable on high-resolution laptop screens
        }
        self.fonts.set_size(px);
        self.window = Some(window);
        self.surface = Some(surface);
        self._context = Some(context);
        if api::serve(&self.session, self.proxy.clone()).is_err() {
            self.toast(
                "Control commands are unavailable (could not open the local port)".into(),
                ALERT,
            );
        }
        self.start();
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if platform::in_lightsout() {
                    return;
                }
                // agents keep running; `lightsout` brings the window back
                self.save_session();
                if let Some(w) = &self.window {
                    w.set_visible(false);
                }
                self.hidden = true;
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                if focused {
                    if let Some(id) = self.focused() {
                        if let Some(p) = self.panes.get_mut(&id) {
                            p.det.seen();
                        }
                    }
                    self.reload_config_if_changed();
                }
            }
            WindowEvent::Resized(s) => {
                self.size = (s.width.max(1), s.height.max(1));
                self.resize_all();
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = position;
                self.mouse_move();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } if self.screen_off => {
                // a click or tap wakes the screen (and does nothing else)
                platform::screen(true);
                self.screen_off = false;
                self.dirty = true;
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => self.mouse_down(button),
                ElementState::Released => self.mouse_up(),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                self.last_input = Instant::now();
                let lines = wheel_lines(&mut self.wheel_rest, delta, self.fonts.cell_h as f64);
                if lines != 0 {
                    self.on_wheel(lines);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.key_press(&event.logical_key, event.text.as_deref());
            }
            WindowEvent::RedrawRequested => {
                self.draw();
                self.dirty = false;
                self.last_draw = Instant::now();
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, event: UserEvent) {
        self.on_user_event(event);
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.quit {
            el.exit();
            return;
        }
        let next = self.tick();
        if std::mem::take(&mut self.redraw) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        el.set_control_flow(match next {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

impl App {
    /// Something happened in a pane, the control port or a bar item.
    pub(crate) fn on_user_event(&mut self, event: UserEvent) {
        let (id, ev) = match event {
            UserEvent::Api(req) => return self.api(req),
            UserEvent::BarText(n, s) => {
                if let Some(slot) = self.bar_texts.get_mut(n) {
                    *slot = s;
                    self.dirty = true;
                }
                return;
            }
            UserEvent::Term(id, ev) => (id, ev),
            // the terminal front-end handles these itself
            UserEvent::Tty(_) | UserEvent::Attach(..) | UserEvent::ClientGone(_) => return,
        };
        if !self.panes.contains_key(&id) {
            return;
        }
        match ev {
            TermEvent::Wakeup => {
                if let Some(p) = self.panes.get_mut(&id) {
                    p.det.on_output();
                }
                self.dirty = true;
            }
            TermEvent::Title(s) if s.starts_with("lightsout-room") => {
                let ri = self.room_of(id);
                if let Some(r) = ri.and_then(|ri| self.rooms.get_mut(ri)) {
                    if let Some(json) = s.strip_prefix("lightsout-room:") {
                        r.remote_agents = serde_json::from_str(json).unwrap_or_default();
                        r.offline = false;
                    } else if s == "lightsout-room-offline" {
                        r.offline = true;
                    }
                }
                self.dirty = true;
            }
            TermEvent::Title(s) => {
                let lower = s.to_lowercase();
                if let Some(p) = self.panes.get_mut(&id) {
                    // shells often set their program path as the title; the folder name is more useful
                    p.title = if lower.ends_with(".exe") || lower.contains(r"\system32\") {
                        p.cwd
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(s)
                    } else {
                        s
                    };
                }
                self.dirty = true;
            }
            TermEvent::Bell => {
                if let Some(p) = self.panes.get_mut(&id) {
                    p.det.on_bell();
                }
                self.status_tick();
            }
            TermEvent::PtyWrite(s) => {
                if let Some(p) = self.panes.get(&id) {
                    p.write_str(s);
                }
            }
            TermEvent::TextAreaSizeRequest(f) => {
                if let Some(p) = self.panes.get(&id) {
                    p.write_str(f(tab::window_size(
                        p.size,
                        self.fonts.cell_w,
                        self.fonts.cell_h,
                    )));
                }
            }
            TermEvent::ColorRequest(idx, f) => {
                let c = if idx < 16 {
                    PALETTE[idx]
                } else if idx == 257 {
                    self.theme.bg
                } else {
                    self.theme.fg
                };
                if let Some(p) = self.panes.get(&id) {
                    p.write_str(f(Rgb {
                        r: (c >> 16) as u8,
                        g: (c >> 8) as u8,
                        b: c as u8,
                    }));
                }
            }
            TermEvent::ClipboardStore(_, s) => {
                if let Some(c) = self.clipboard.as_mut() {
                    let _ = c.set_text(s);
                }
            }
            // A program in a pane may put text on the clipboard, but never read it back:
            // a file being shown or a server you're logged into could quietly take a copied password.
            TermEvent::ClipboardLoad(..) => {}
            TermEvent::ChildExit(_) | TermEvent::Exit => self.close_pane(id),
            _ => {}
        }
    }

    /// Everything that is due now (panes, alerts, saving, redraws); answers when to look again.
    pub(crate) fn tick(&mut self) -> Option<Instant> {
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        let mut soon = |t: Instant| next = Some(next.map_or(t, |n: Instant| n.min(t)));

        if let Some(rx) = &self.wifi_scan {
            match rx.try_recv() {
                Ok(nets) => {
                    self.wifi_scan = None;
                    {
                        let bars = ["▁   ", "▁▂  ", "▁▂▄ ", "▁▂▄▆", "▁▂▄▆"];
                        let mut items: Vec<Item> = nets
                            .iter()
                            .enumerate()
                            .map(|(i, n)| Item {
                                label: format!(
                                    "{}  {}{}",
                                    bars[n.signal.min(4) as usize],
                                    n.ssid,
                                    if n.connected { "   (connected)" } else { "" }
                                ),
                                hint: if n.secure {
                                    "locked".into()
                                } else {
                                    "open".into()
                                },
                                action: Action::WifiJoin(i),
                            })
                            .collect();
                        items.push(Item {
                            label: "Other network...".into(),
                            hint: "type a name".into(),
                            action: Action::WifiOther,
                        });
                        self.wifi_nets = nets;
                        self.overlay = Overlay::Palette {
                            title: "Wi-Fi".into(),
                            items,
                            filter: String::new(),
                            sel: 0,
                            picked: false,
                            top: 0,
                        };
                    }
                    self.dirty = true;
                }
                Err(mpsc::TryRecvError::Empty) => soon(now + Duration::from_millis(300)),
                Err(_) => self.wifi_scan = None,
            }
        }
        if let Some(rx) = &self.wifi_join {
            match rx.try_recv() {
                Ok(r) => {
                    self.wifi_join = None;
                    match r {
                        Ok(ssid) => self.toast(format!("Connected to {ssid}"), DONE),
                        Err(e) => self.toast(format!("Wi-Fi: {e}"), ALERT),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => soon(now + Duration::from_millis(300)),
                Err(_) => self.wifi_join = None,
            }
        }
        if let Some(rx) = &self.commands_rx {
            match rx.try_recv() {
                Ok(map) => {
                    self.commands = map;
                    self.commands_rx = None;
                    // a program started or ended in a tab: a crash from now on brings it back
                    self.save_session();
                }
                Err(mpsc::TryRecvError::Empty) => soon(now + Duration::from_millis(500)),
                Err(_) => self.commands_rx = None,
            }
        } else if self.commands_stale {
            self.commands_stale = false;
            let pids: Vec<u32> = self.panes.values().filter_map(|p| p.shell_pid).collect();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(platform::running_in_tabs(&pids));
            });
            self.commands_rx = Some(rx);
            soon(now + Duration::from_millis(500));
        }
        if let Some(rx) = &self.switching {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.switching = None;
                    // Windows restarts now (Linux and a Mac logging out close the app below)
                    if cfg!(windows) {
                        self.begin_closing();
                    }
                    if cfg!(target_os = "linux") || platform::mac_logging_out() {
                        // Linux: the desktop is stopping; Mac: the background service takes
                        // over and you're logged out. Either way these tabs reopen there.
                        std::process::exit(0);
                    }
                    if cfg!(target_os = "macos") {
                        // Mac staying signed in: this window keeps the agents, full screen, screen off
                        if let Some(w) = &self.window {
                            w.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                        platform::screen(false);
                    }
                    self.toast("Restarting into Lights Out...".into(), self.theme.accent);
                }
                Ok(Err(e)) => {
                    self.switching = None;
                    self.toast(e, ALERT);
                }
                Err(_) => soon(now + Duration::from_millis(200)),
            }
        }
        // agent states: checked only while something is happening (nothing runs when everything is idle)
        let busy = !self.waiters.is_empty()
            || self
                .panes
                .values()
                .any(|p| p.det.recently_active() || p.det.state == State::Working);
        if busy {
            if now.duration_since(self.status_at) >= Duration::from_millis(500) {
                self.status_tick();
            }
            soon(self.status_at + Duration::from_millis(500));
            if let Some(d) = self.waiters.iter().map(|w| w.deadline).min() {
                soon(d);
            }
        }
        // the battery, also with the screen off: in Lights Out nothing else watches it
        self.refresh_battery();
        if watches_battery() {
            soon(self.battery_at + Duration::from_secs(20));
        }
        let before = self.toasts.len();
        self.toasts
            .retain(|t| t.at.elapsed() < Duration::from_secs(6));
        if self.toasts.len() != before {
            self.dirty = true;
        }
        if let Some(t) = self
            .toasts
            .iter()
            .map(|t| t.at + Duration::from_secs(6))
            .min()
        {
            soon(t);
        }
        // settings: picked up when the file changes (checked every few seconds, only while the window is in front)
        if self.window_focused && !self.hidden && !self.screen_off {
            if self.cfg_checked.elapsed() >= Duration::from_secs(3) {
                self.reload_config_if_changed();
            }
            soon(self.cfg_checked + Duration::from_secs(3));
        }
        // keep the restore point fresh: a room renamed, a tab opened or closed, a folder or a
        // program changed is saved within 2 s (a crash, an update or a forced close then
        // loses nothing); the screens' text once a minute while agents work
        self.shape_check |= self.dirty;
        if self.shape_check {
            let due = self.shape_at + Duration::from_secs(2);
            if now >= due {
                self.shape_at = now;
                self.shape_check = false;
                if self.shape() != self.saved_shape {
                    self.save_session();
                }
            } else {
                soon(due);
            }
        }
        if busy && self.saved_at.elapsed() >= Duration::from_secs(60) {
            self.save_session();
        }
        if self.dirty && !self.screen_off && !self.hidden {
            let frame = self.last_draw
                + if self.on_battery && self.last_input.elapsed() > ACTIVE {
                    FRAME_ON_BATTERY
                } else {
                    FRAME
                };
            if now >= frame {
                self.redraw = true;
            } else {
                soon(frame);
            }
        }
        next
    }
}

// ---------------------------------------------------------------- command line

const HELP: &str = "\
Lights Out: the terminal for AI agents.

  lightsout                       run Lights Out right here, in this terminal
                                  (Ctrl+B then a key: P actions, T tab, W close, N room; F1 = actions)
  lightsout --window              open it in its own window instead
  lightsout open USER@HOST        Lights Out on a server, over SSH (agents live there)
  lightsout room add [NAME]       a new room
  lightsout room add USER@HOST [NAME]   a room that lives on that server
  lightsout room list | remove NAME
  lightsout --session NAME        a separate, named set of tabs
  lightsout --plain               start with plugins off (also Ctrl+Shift+F12 at any time)
  lightsout plugin install OWNER/REPO | link FOLDER | list | update | remove ID | on ID | off ID
                                  plugins: new commands, keys and programs that run on events
  lightsout update                the newest version (your settings, logins and programs stay)
  lightsout uninstall             remove Lights Out from this computer

Control the running app (also for agents; details: `lightsout skill`):
  lightsout commands              everything Lights Out can do, with its keys
  lightsout do COMMAND [ARG]      do one of them, as its key or the menu does (`lightsout do room.rename`)
  lightsout list | new | send | read | wait | explain | focus | close | notify | screen | power | switch | quit
  lightsout report-session ID     (for an agent's hook: the conversation in its tab)
";

/// One word of a command, quoted for the shell that runs it (PowerShell on Windows, sh elsewhere).
fn shell_word(w: &str) -> String {
    let plain = !w.is_empty()
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c));
    if plain {
        w.to_string()
    } else if cfg!(windows) {
        format!("'{}'", w.replace('\'', "''"))
    } else {
        format!("'{}'", w.replace('\'', r"'\''"))
    }
}

fn client(session: &str, args: &[String]) -> i32 {
    let rest: Vec<String> = args[1..].to_vec();
    let flag = |name: &str| rest.iter().position(|a| a == name);
    let value = |name: &str| flag(name).and_then(|i| rest.get(i + 1)).cloned();
    let mut positional: Vec<String> = Vec::new();
    let mut skip = false;
    for a in &rest {
        if skip {
            skip = false;
            continue;
        }
        if a == "--" {
            break;
        }
        if ["--cwd", "--lines", "--until", "--timeout"].contains(&a.as_str()) {
            skip = true;
            continue;
        }
        if !a.starts_with("--") {
            positional.push(a.clone());
        }
    }
    // the words after `--` keep their quoting: `-- sh -c 'echo hi; sleep 5'` stays one command
    let after_dashes = flag("--").map(|i| {
        rest[i + 1..]
            .iter()
            .map(|w| shell_word(w))
            .collect::<Vec<_>>()
            .join(" ")
    });
    let pane = |n: usize| -> Value {
        match positional.get(n).map(String::as_str) {
            Some("self") | None => json!(std::env::var("LIGHTSOUT_PANE").unwrap_or_default()),
            Some(p) => json!(p),
        }
    };
    let split = if flag("--right").is_some() {
        Some("right")
    } else if flag("--down").is_some() {
        Some("down")
    } else {
        None
    };
    let (cmd, a) = match args[0].as_str() {
        // the command's name as it is, the rest of a command line keeps its quoting
        "do" if !rest.is_empty() => {
            let line = std::iter::once(rest[0].clone())
                .chain(rest[1..].iter().map(|w| shell_word(w)))
                .collect::<Vec<_>>()
                .join(" ");
            ("do", json!({ "command": line }))
        }
        "commands" => ("commands", json!({})),
        "list" => ("list", json!({})),
        "new" => (
            "new",
            json!({ "cwd": value("--cwd"), "command": after_dashes.or_else(|| (!positional.is_empty()).then(|| positional.join(" "))), "split": split }),
        ),
        "send" => (
            "send",
            json!({ "pane": pane(0), "text": positional.iter().skip(1).cloned().collect::<Vec<_>>().join(" "), "enter": flag("--no-enter").is_none() }),
        ),
        "read" => (
            "read",
            json!({ "pane": pane(0), "lines": value("--lines").and_then(|v| v.parse::<u64>().ok()).unwrap_or(60) }),
        ),
        "wait" => (
            "wait",
            json!({ "pane": pane(0), "until": value("--until"), "timeout": value("--timeout").and_then(|v| v.parse::<u64>().ok()).unwrap_or(600) }),
        ),
        "explain" => ("explain", json!({ "pane": pane(0) })),
        "focus" => ("focus", json!({ "pane": pane(0) })),
        "close" => ("close", json!({ "pane": pane(0) })),
        "notify" => ("notify", json!({ "text": positional.join(" ") })),
        "screen" => (
            "screen",
            json!({ "on": positional.first().map(|s| s != "off").unwrap_or(true) }),
        ),
        "power" => ("power", json!({})),
        "switch" | "windows" => ("switch", json!({})),
        "handback" => ("handback", json!({})),
        "quit" => ("quit", json!({})),
        _ => {
            eprint!("{HELP}");
            return 2;
        }
    };
    match api::call(session, cmd, a) {
        Ok(v) => {
            match cmd {
                "commands" => {
                    println!("{:<30} {:<26} WHAT IT DOES", "COMMAND", "KEYS");
                    for c in v["commands"].as_array().cloned().unwrap_or_default() {
                        let name = match c["arg"].as_str() {
                            Some(a) if !a.is_empty() => {
                                format!("{} {a}", c["id"].as_str().unwrap_or(""))
                            }
                            _ => c["id"].as_str().unwrap_or("").to_string(),
                        };
                        let keys: Vec<&str> = c["keys"]
                            .as_array()
                            .map(|k| k.iter().filter_map(|x| x.as_str()).collect())
                            .unwrap_or_default();
                        println!(
                            "{name:<30} {:<26} {}",
                            keys.join(", "),
                            c["title"].as_str().unwrap_or("")
                        );
                    }
                }
                // everything about each pane (a floating one's place and size too), for programs
                "list" if flag("--json").is_some() => println!("{}", v["panes"]),
                "list" => {
                    println!(
                        "{:<5} {:<12} {:<4} {:<10} {:<20} FOLDER",
                        "PANE", "ROOM", "TAB", "STATE", "TITLE"
                    );
                    for p in v["panes"].as_array().cloned().unwrap_or_default() {
                        println!(
                            "{:<5} {:<12} {:<4} {:<10} {:<20} {}",
                            p["id"],
                            p["room"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .take(12)
                                .collect::<String>(),
                            p["tab"],
                            p["state"].as_str().unwrap_or(""),
                            p["title"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .take(20)
                                .collect::<String>(),
                            p["cwd"].as_str().unwrap_or("")
                        );
                    }
                }
                "new" => println!("{}", v["id"]),
                "read" => println!("{}", v["text"].as_str().unwrap_or("")),
                "wait" => println!(
                    "{}{}",
                    v["state"].as_str().unwrap_or(""),
                    if v["timeout"].as_bool() == Some(true) {
                        " (timed out)"
                    } else {
                        ""
                    }
                ),
                "explain" => println!(
                    "{}: {}",
                    v["state"].as_str().unwrap_or(""),
                    v["reason"].as_str().unwrap_or("")
                ),
                "power" => match v["percent"].as_u64() {
                    Some(p) => println!(
                        "battery {p}%{}{}{}",
                        v["hours_left"]
                            .as_f64()
                            .map(|h| format!(", {} left", time_left(h as f32)))
                            .unwrap_or_default(),
                        v["watts"]
                            .as_f64()
                            .map(|w| format!(", drawing {w:.1} W"))
                            .unwrap_or_default(),
                        if v["charging"].as_bool() == Some(true) {
                            ", charging"
                        } else {
                            ""
                        }
                    ),
                    None => println!("no battery"),
                },
                _ => {}
            }
            0
        }
        Err(e) => {
            eprintln!("lightsout: {e}");
            1
        }
    }
}

/// `lightsout` (command line): runs a command, or opens the app.
pub fn cli_entry() {
    let (session, rest) = parse_args();
    // the background copy that owns the agents for terminal views (started by `lightsout`)
    if rest.first().map(String::as_str) == Some("--server") {
        alacritty_terminal::tty::setup_env();
        platform::lid_keep_running();
        let code = tui::serve(&session);
        platform::lid_restore();
        std::process::exit(code);
    }
    // `--room NAME`: show just that room (a room on this machine, seen from another one)
    // `--embed`: whatever room is open (a server shown as a room on another computer)
    let room_arg: Option<String> = if rest.iter().any(|a| a == "--embed") {
        Some(".".into())
    } else {
        rest.iter()
            .position(|a| a == "--room")
            .and_then(|i| rest.get(i + 1))
            .map(|r| {
                r.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect()
            })
    };
    if rest.first().map(String::as_str) == Some("open") {
        let Some(target) = rest.get(1) else {
            eprintln!("lightsout: which server? lightsout open you@server");
            std::process::exit(2);
        };
        let there = match room_arg.as_deref() {
            Some(".") => "sh -lc 'exec lightsout --embed'".to_string(),
            Some(r) => format!("sh -lc 'exec lightsout --room {r}'"),
            None => "sh -lc 'exec lightsout'".to_string(),
        };
        loop {
            let status = std::process::Command::new("ssh")
                .args(["-t", target, &there])
                .status();
            match status {
                Ok(s) if s.code() == Some(127) => {
                    eprintln!("lightsout: not installed on {target}. There: curl -fsSL https://lightsout.sh | sh");
                    std::process::exit(1);
                }
                // 255: ssh couldn't connect or the connection dropped
                Ok(s) if s.code() != Some(255) => std::process::exit(s.code().unwrap_or(0)),
                Err(e) if room_arg.is_none() => {
                    eprintln!("lightsout: could not start ssh: {e}");
                    std::process::exit(1);
                }
                _ if room_arg.is_none() => std::process::exit(255),
                _ => {
                    // a room on that machine: keep trying (the sidebar dims it meanwhile)
                    print!("\x1b]2;lightsout-room-offline\x07");
                    println!("Can't reach {target}. Trying again in 5 s (Ctrl+C stops).");
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            }
        }
    }
    let window = rest.iter().any(|a| a == "--window" || a == "-w");
    let view_only = (room_arg.as_deref() == Some(".") && rest.len() == 1)
        || (room_arg.is_some() && rest.len() == 2);
    if rest.is_empty() || window || view_only {
        use std::io::IsTerminal;
        let in_terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        if !window && in_terminal {
            if std::env::var_os("LIGHTSOUT_PANE").is_some() {
                eprintln!("lightsout: this is already inside Lights Out");
                std::process::exit(1);
            }
            // typed in a terminal: show Lights Out right here, like tmux. The agents live in a
            // background copy, so closing this terminal (or Ctrl+B D) leaves them running.
            if let Some(code) = tui::attach(&session, room_arg.as_deref()) {
                std::process::exit(code);
            }
            // no background copy yet; is the window open?
            if api::call(&session, "show", json!({})).is_ok() {
                return;
            }
            platform::mac_return(&session);
            if let Some(code) = tui::start_and_attach(&session, room_arg.as_deref()) {
                std::process::exit(code);
            }
            eprintln!("lightsout: could not start in the background; opening the window instead");
        } else if api::call(&session, "show", json!({})).is_ok() {
            // already open: bring it up instead of a second copy
            return;
        }
        // open the window without tying up this terminal
        let mut app = std::env::current_exe().unwrap_or_default();
        app.set_file_name(if cfg!(windows) {
            "lightsout-app.exe"
        } else {
            "lightsout-app"
        });
        let mut c = std::process::Command::new(app);
        if session != "default" {
            c.args(["--session", &session]);
        }
        if let Err(e) = c.spawn() {
            eprintln!("lightsout: could not open the app: {e}");
            std::process::exit(1);
        }
        return;
    }
    run_command(&session, &rest);
}

fn parse_args() -> (String, Vec<String>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut session = std::env::var("LIGHTSOUT_SESSION").unwrap_or_else(|_| "default".into());
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--session" {
            if let Some(s) = it.next() {
                session = s;
            }
        } else if a == "--plain" {
            // plugins off (the way back when a plugin broke the screen); the app or the
            // background copy started from here inherits it
            std::env::set_var("LIGHTSOUT_PLAIN", "1");
        } else {
            rest.push(a);
        }
    }
    (session, rest)
}

fn run_command(session: &str, rest: &[String]) {
    if let Some(first) = rest.first() {
        platform::attach_console();
        match first.as_str() {
            "skill" => print!("{}", api::SKILL),
            "-h" | "--help" | "help" => print!("{HELP}"),
            "-V" | "--version" | "version" => println!("lightsout {}", env!("CARGO_PKG_VERSION")),
            "uninstall" => std::process::exit(uninstall()),
            "report-session" => std::process::exit(report_session(session, &rest[1..])),
            "update" => std::process::exit(update(session)),
            "room" | "rooms" => std::process::exit(room_cmd(session, &rest[1..])),
            "plugin" | "plugins" => std::process::exit(plugins::cli(session, &rest[1..])),
            "part" => std::process::exit(part_cmd(session, &rest[1..])),
            _ => std::process::exit(client(session, rest)),
        }
    }
}

/// `lightsout part set ID [CONTENT]` / `lightsout part clear ID`: what a plugin's part
/// shows. CONTENT is JSON (see parts.rs) or plain text, after the ID or on stdin. Inside a
/// plugin's program, ID may be the part's own name (the plugin's is added).
fn part_cmd(session: &str, args: &[String]) -> i32 {
    use std::io::{IsTerminal, Read};
    if args.first().map(String::as_str) == Some("list") {
        return match api::call(session, "parts", json!({})) {
            Ok(v) => {
                println!(
                    "{:<24} {:<7} {:>4} {:>4} {:>5} {:>5}  SHOWING",
                    "PART", "SIDE", "X", "Y", "COLS", "ROWS"
                );
                let n = |v: &Value| v.as_i64().unwrap_or(0);
                let line = |id: &str, side: &str, at: &Value, showing: String| {
                    println!(
                        "{id:<24} {side:<7} {:>4} {:>4} {:>5} {:>5}  {showing}",
                        n(&at["x"]),
                        n(&at["y"]),
                        n(&at["cols"]),
                        n(&at["rows"])
                    )
                };
                for p in v["parts"].as_array().cloned().unwrap_or_default() {
                    let pic = &p["picture"];
                    let showing = match p["showing"].as_i64() {
                        _ if pic.is_array() => format!("a picture {}×{}", pic[0], pic[1]),
                        Some(1) => "1 row".into(),
                        Some(r) => format!("{r} rows"),
                        None => "built in".into(),
                    };
                    line(
                        p["id"].as_str().unwrap_or(""),
                        p["side"].as_str().unwrap_or(""),
                        &p["at"],
                        showing,
                    );
                }
                line("(panes)", "", &v["panes"], String::new());
                0
            }
            Err(e) => {
                eprintln!("lightsout: {e}");
                1
            }
        };
    }
    let (Some(what), Some(id)) = (args.first(), args.get(1)) else {
        eprintln!("lightsout part list | set PLUGIN.PART [JSON or text] | image PLUGIN.PART FILE | clear PLUGIN.PART");
        return 2;
    };
    let id = match std::env::var("LIGHTSOUT_PLUGIN_ID") {
        Ok(p) if !id.contains('.') => format!("{p}.{id}"),
        _ => id.clone(),
    };
    let (cmd, args) = match what.as_str() {
        "set" => {
            let mut text = args[2..].join(" ");
            if text.is_empty() && !std::io::stdin().is_terminal() {
                let _ = std::io::stdin().read_to_string(&mut text);
            }
            let content = serde_json::from_str::<Value>(text.trim())
                .unwrap_or_else(|_| Value::String(text.trim_end().to_string()));
            ("part-set", json!({ "id": id, "content": content }))
        }
        "clear" => ("part-clear", json!({ "id": id })),
        "image" => {
            let Some(file) = args.get(2) else {
                eprintln!("lightsout part image PLUGIN.PART FILE   (a PNG or PPM picture)");
                return 2;
            };
            // the app may run in another folder: the file's full path
            let file = std::fs::canonicalize(file).unwrap_or_else(|_| PathBuf::from(file));
            let file = file
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_string();
            ("part-image", json!({ "id": id, "file": file }))
        }
        _ => {
            eprintln!("lightsout part list | set PLUGIN.PART [JSON or text] | image PLUGIN.PART FILE | clear PLUGIN.PART");
            return 2;
        }
    };
    match api::call(session, cmd, args) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("lightsout: {e}");
            1
        }
    }
}

/// `lightsout room add | list | remove`: rooms here, and rooms that live on a server.
fn room_cmd(session: &str, args: &[String]) -> i32 {
    let on = args
        .iter()
        .position(|a| a == "--on")
        .and_then(|i| args.get(i + 1))
        .cloned();
    // the words after add/remove, without `--on X`
    let mut words: Vec<&String> = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--on" {
            it.next();
        } else {
            words.push(a);
        }
    }
    let running = |cmd: &str, a: Value| api::call(session, cmd, a);
    match args.first().map(String::as_str) {
        Some("add") => {
            // on a server: `me@server`, or `NAME --on <ssh name>`
            let server = on.clone().or_else(|| {
                words
                    .first()
                    .filter(|w| w.contains('@'))
                    .map(|w| w.to_string())
            });
            if let Some(ssh) = server {
                let name = words
                    .iter()
                    .find(|w| **w != &ssh)
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| session::server_name(&ssh));
                let mut all = session::server_rooms();
                all.retain(|m| m.name != name);
                all.push(session::ServerRoom {
                    name: name.clone(),
                    ssh: ssh.clone(),
                });
                if let Err(e) = session::save_server_rooms(&all) {
                    eprintln!("lightsout: couldn't save: {e}");
                    return 1;
                }
                let _ = running("server-rooms", json!({}));
                println!("room {name} on {ssh}: its agents run there");
                return 0;
            }
            let name = words.first().map(|w| w.to_string()).unwrap_or_default();
            match running("room-add", json!({ "name": name })) {
                Ok(_) => 0,
                Err(_) => {
                    eprintln!("lightsout: Lights Out isn't open (type lightsout first)");
                    1
                }
            }
        }
        Some("remove") | Some("rm") => {
            let Some(name) = words.first().map(|w| w.to_string()) else {
                eprintln!("lightsout: lightsout room remove NAME");
                return 2;
            };
            let mut all = session::server_rooms();
            let before = all.len();
            all.retain(|m| m.name != name);
            if all.len() != before {
                if let Err(e) = session::save_server_rooms(&all) {
                    eprintln!("lightsout: couldn't save: {e}");
                    return 1;
                }
                let _ = running("server-rooms", json!({}));
                println!("removed {name} here (its agents keep running on the server)");
                return 0;
            }
            match running("room-remove", json!({ "name": name })) {
                Ok(v) if v["ok"].as_bool() == Some(true) => 0,
                Ok(_) => {
                    eprintln!("lightsout: no room called {name}");
                    1
                }
                Err(_) => {
                    eprintln!("lightsout: Lights Out isn't open");
                    1
                }
            }
        }
        Some("list") | Some("ls") | None => {
            match running("rooms", json!({})) {
                Ok(v) => {
                    for r in v["rooms"].as_array().cloned().unwrap_or_default() {
                        match r["server"].as_str() {
                            Some(s) => println!("{}  (on {s})", r["name"].as_str().unwrap_or("")),
                            None => println!("{}", r["name"].as_str().unwrap_or("")),
                        }
                    }
                }
                Err(_) => {
                    for m in session::server_rooms() {
                        println!("{}  (on {})", m.name, m.ssh);
                    }
                }
            }
            0
        }
        Some(other) => {
            eprintln!("lightsout: room {other}? use add, list or remove");
            2
        }
    }
}

/// `lightsout report-session [ID]`: an agent's hook (see platform::agent_hooks) tells its tab
/// which conversation it has: the ID, or the hook's details on stdin (`session_id`). It prints
/// nothing and always succeeds: the agent gives what a hook prints to its model, and runs on
/// outside Lights Out just the same.
fn report_session(session: &str, args: &[String]) -> i32 {
    use std::io::{IsTerminal, Read};
    let Ok(pane) = std::env::var("LIGHTSOUT_PANE") else {
        return 0;
    };
    let id = args.first().cloned().or_else(|| {
        let mut text = String::new();
        let stdin = std::io::stdin();
        if stdin.is_terminal() || stdin.lock().read_to_string(&mut text).is_err() {
            return None;
        }
        let v: Value = serde_json::from_str(&text).ok()?;
        ["session_id", "sessionId", "conversation_id", "thread_id"]
            .iter()
            .find_map(|k| v[*k].as_str().map(str::to_string))
    });
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        let _ = api::call(session, "report-session", json!({ "pane": pane, "id": id }));
    }
    0
}

/// Removes Lights Out from this computer (asks for admin rights).
fn uninstall() -> i32 {
    // the agents' hooks first: they live in your own settings, which the uninstaller (as
    // admin or root) doesn't touch
    platform::agent_hooks(false);
    let status = if cfg!(windows) {
        let script = platform::lightsout_dir().join("bin").join("uninstall.ps1");
        let cmd = format!(
            "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"{}\"'",
            script.display()
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &cmd])
            .status()
    } else {
        std::process::Command::new("sudo")
            .args(["sh", "/usr/local/lib/lightsout/uninstall.sh"])
            .status()
    };
    match status {
        Ok(s) if s.success() => 0,
        _ => {
            eprintln!("lightsout: uninstall did not finish");
            1
        }
    }
}

/// `lightsout update`: runs the installer again, from the channel it was installed from
/// (stable or test builds; the installer saved its address). Settings, logins, programs and
/// tabs stay. In Lights Out mode the update is asked for here and installed by Windows at
/// the restart (cleanup.ps1), which comes straight back to Lights Out.
fn update(session: &str) -> i32 {
    if platform::image() {
        let dir = platform::lightsout_dir().join("update");
        if std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(dir.join("requested"), ""))
            .is_err()
        {
            eprintln!("lightsout: could not ask for the update");
            return 1;
        }
        println!(
            "Updating: Lights Out restarts, Windows installs the update and comes straight back."
        );
        return client(session, &["switch".to_string()]);
    }
    let file = if cfg!(windows) {
        platform::lightsout_dir().join("bin").join("installer-url")
    } else {
        std::path::PathBuf::from("/usr/local/lib/lightsout/installer-url")
    };
    let Ok(site) = std::fs::read_to_string(&file) else {
        eprintln!(
            "lightsout: no installer address in {} (install once with the installer)",
            file.display()
        );
        return 1;
    };
    let site = site.trim();
    let status = if cfg!(windows) {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!("irm {site} | iex"),
            ])
            .status()
    } else {
        std::process::Command::new("sh")
            .args(["-c", &format!("curl -fsSL {site} | sh")])
            .status()
    };
    match status {
        Ok(s) if s.success() => 0,
        _ => {
            eprintln!("lightsout: the update did not finish");
            1
        }
    }
}

/// `lightsout-app`: the window itself.
pub fn main_entry() {
    let (session, rest) = parse_args();
    if rest.first().map(String::as_str) == Some("--headless") {
        alacritty_terminal::tty::setup_env();
        return headless::run(&session);
    }
    // signing in (Windows, Mac): open only when coming back from Lights Out
    if rest.first().map(String::as_str) == Some("--if-returning") && !platform::returning() {
        return;
    }
    let rest: Vec<String> = rest.into_iter().filter(|a| a != "--if-returning").collect();
    if !rest.is_empty() {
        return run_command(&session, &rest);
    }
    // Mac: back from Lights Out, take the tabs from the background service first
    platform::mac_return(&session);
    // already running? bring that window back instead of opening a second one
    if api::call(&session, "show", json!({})).is_ok() {
        return;
    }
    platform::fresh_environment();
    alacritty_terminal::tty::setup_env();
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let mut app = App::new(Waker::Window(event_loop.create_proxy()), session);
    platform::lid_keep_running();
    let _ = event_loop.run_app(&mut app);
    platform::lid_restore();
}

/// Shows a drawn frame. The compositor is told only the rows that changed since the last one
/// (a moving spinner is a thin strip, not the whole screen); an unchanged frame isn't sent.
fn present(buffer: softbuffer::Buffer<'_, Rc<Window>, Rc<Window>>, shown: &mut Vec<u32>, w: u32) {
    let Some((top, bottom)) = changed_rows(&buffer, shown, w as usize) else {
        return;
    };
    shown.clear();
    shown.extend_from_slice(&buffer);
    let (Some(width), Some(height)) = (
        NonZeroU32::new(w),
        NonZeroU32::new((bottom - top + 1) as u32),
    ) else {
        return;
    };
    let _ = buffer.present_with_damage(&[softbuffer::Rect {
        x: 0,
        y: top as u32,
        width,
        height,
    }]);
}

/// The first and last row (of `w` pixels) that differ; none for the same frame. A frame of
/// another size differs everywhere.
fn changed_rows(new: &[u32], old: &[u32], w: usize) -> Option<(usize, usize)> {
    let rows = new.len().checked_div(w).filter(|r| *r > 0)?;
    if new.len() != old.len() {
        return Some((0, rows - 1));
    }
    let same = |i: usize| new[i * w..(i + 1) * w] == old[i * w..(i + 1) * w];
    Some((
        (0..rows).find(|&i| !same(i))?,
        (0..rows).rev().find(|&i| !same(i))?,
    ))
}

/// Lights Out on a PC (Windows off, or the Linux desktop stopped): nothing else acts on an
/// empty battery there. (A Mac keeps its own; Windows and desktops have theirs.)
fn watches_battery() -> bool {
    cfg!(target_os = "linux") && platform::in_lightsout()
}

#[derive(Debug, PartialEq)]
enum LowBattery {
    Fine,
    Warn,
    ShutDown,
}

/// What a battery reading asks for: a warning once at LOW_BATTERY, the clean shut-down at
/// EMPTY_BATTERY; nothing while charging.
fn low_battery(percent: u8, charging: bool, warned: bool) -> LowBattery {
    if charging {
        LowBattery::Fine
    } else if percent <= EMPTY_BATTERY {
        LowBattery::ShutDown
    } else if percent <= LOW_BATTERY && !warned {
        LowBattery::Warn
    } else {
        LowBattery::Fine
    }
}

#[cfg(test)]
mod low_battery_tests {
    use super::{low_battery, LowBattery::*};
    #[test]
    fn warns_once_then_shuts_down_and_never_while_charging() {
        assert_eq!(low_battery(50, false, false), Fine);
        assert_eq!(low_battery(10, false, false), Warn);
        assert_eq!(low_battery(9, false, true), Fine);
        assert_eq!(low_battery(3, false, true), Fine);
        assert_eq!(low_battery(2, false, true), ShutDown);
        assert_eq!(low_battery(2, false, false), ShutDown);
        assert_eq!(low_battery(1, true, true), Fine);
    }
}

/// 0.75 h -> "45m", 1.67 h -> "1h40".
fn time_left(hours: f32) -> String {
    let m = (hours * 60.0).round() as u32;
    if m < 60 {
        format!("{m}m")
    } else {
        format!("{}h{:02}", m / 60, m % 60)
    }
}

/// Lines to scroll (> 0 is up). A touchpad moves a few pixels at a time: the parts of a
/// line add up in `rest` instead of each rounding down to nothing.
fn wheel_lines(rest: &mut f64, delta: MouseScrollDelta, cell_h: f64) -> i32 {
    *rest += match delta {
        MouseScrollDelta::LineDelta(_, y) => y as f64 * 3.0,
        MouseScrollDelta::PixelDelta(p) => p.y / cell_h,
    };
    let lines = rest.trunc();
    *rest -= lines;
    lines as i32
}

/// A restored tab's last screen, shown once, dimmed; not for an agent that continues
/// (it draws its own conversation again, and the old text would sit under it).
fn screen_to_show<'a>(screen: Option<&'a str>, resume: Option<&str>) -> Option<&'a str> {
    screen.filter(|s| resume.is_none() && !s.trim().is_empty())
}

/// The web link (http or https) on a line of the screen around the column clicked: the word
/// there (up to a space, quote, bracket...), from its `http`, without a full stop or comma
/// after it.
fn link_in(line: &[char], col: usize) -> Option<String> {
    let stop = |c: char| c.is_whitespace() || "\"'<>`()[]{}|".contains(c);
    if col >= line.len() || stop(line[col]) {
        return None;
    }
    let mut start = col;
    while start > 0 && !stop(line[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < line.len() && !stop(line[end]) {
        end += 1;
    }
    let word: String = line[start..end].iter().collect();
    let from = word.find("https://").or_else(|| word.find("http://"))?;
    // the click is on the link itself, not on what comes before it in the same word
    if col < start + word[..from].chars().count() {
        return None;
    }
    let link = word[from..].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    (link.len() > "https://".len()).then(|| link.to_string())
}

#[cfg(test)]
mod link_tests {
    fn at(s: &str, col: usize) -> Option<String> {
        super::link_in(&s.chars().collect::<Vec<_>>(), col)
    }

    #[test]
    fn the_link_under_the_click() {
        let s = "see https://github.com/a/b/pull/3, then (http://x.io/q?a=1&b=2).";
        assert_eq!(at(s, 10).as_deref(), Some("https://github.com/a/b/pull/3"));
        assert_eq!(at(s, 45).as_deref(), Some("http://x.io/q?a=1&b=2"));
        assert_eq!(at(s, 1), None);
        assert_eq!(
            at("url=https://a.io/x", 6).as_deref(),
            Some("https://a.io/x")
        );
        assert_eq!(at("url=https://a.io/x", 1), None);
        assert_eq!(at("https://", 2), None);
        assert_eq!(at("short", 99), None);
    }
}

#[cfg(test)]
mod screen_tests {
    #[test]
    fn old_screen_only_for_tabs_that_dont_redraw_themselves() {
        assert_eq!(
            super::screen_to_show(Some("$ ls\nfile"), None),
            Some("$ ls\nfile")
        );
        assert_eq!(
            super::screen_to_show(Some("chat text"), Some("claude --resume x")),
            None
        );
        assert_eq!(super::screen_to_show(Some("  \n "), None), None);
    }
}

#[cfg(test)]
mod present_tests {
    #[test]
    fn only_the_rows_that_changed_are_sent() {
        let old = vec![0u32; 4 * 5]; // 4 pixels wide, 5 rows
        let mut new = old.clone();
        assert_eq!(super::changed_rows(&new, &old, 4), None); // the same frame: nothing to send
        new[4 * 2 + 1] = 7; // row 2
        new[4 * 3] = 7; // row 3
        assert_eq!(super::changed_rows(&new, &old, 4), Some((2, 3)));
        assert_eq!(super::changed_rows(&new, &[], 4), Some((0, 4))); // first frame or resized: all
        assert_eq!(super::changed_rows(&new, &old, 0), None);
    }
}

#[cfg(test)]
mod time_left_tests {
    #[test]
    fn time_left_reads_short() {
        assert_eq!(super::time_left(0.75), "45m");
        assert_eq!(super::time_left(1.0 + 40.0 / 60.0), "1h40");
        assert_eq!(super::time_left(6.1), "6h06");
    }
}

#[cfg(test)]
mod wheel_tests {
    use winit::dpi::PhysicalPosition;
    use winit::event::MouseScrollDelta::{LineDelta, PixelDelta};

    #[test]
    fn touchpad_steps_add_up_to_lines() {
        let mut rest = 0.0;
        // a 20 px line, the touchpad moving 5 px at a time: a line every 4 steps
        let moved: i32 = (0..12)
            .map(|_| {
                super::wheel_lines(&mut rest, PixelDelta(PhysicalPosition::new(0.0, 5.0)), 20.0)
            })
            .sum();
        assert_eq!(moved, 3);
        let back: i32 = (0..12)
            .map(|_| {
                super::wheel_lines(
                    &mut rest,
                    PixelDelta(PhysicalPosition::new(0.0, -5.0)),
                    20.0,
                )
            })
            .sum();
        assert_eq!(back, -3);
    }

    #[test]
    fn a_wheel_notch_is_three_lines() {
        let mut rest = 0.0;
        assert_eq!(super::wheel_lines(&mut rest, LineDelta(0.0, 1.0), 20.0), 3);
        assert_eq!(
            super::wheel_lines(&mut rest, LineDelta(0.0, -1.0), 20.0),
            -3
        );
        // a precision touchpad on Windows sends parts of a notch
        assert_eq!(
            (0..4)
                .map(|_| super::wheel_lines(&mut rest, LineDelta(0.0, 0.25), 20.0))
                .sum::<i32>(),
            3
        );
    }
}

#[cfg(test)]
mod quoting_tests {
    #[test]
    fn words_keep_their_quoting() {
        assert_eq!(super::shell_word("claude"), "claude");
        assert_eq!(super::shell_word("echo hi; sleep 5"), "'echo hi; sleep 5'");
        if cfg!(windows) {
            assert_eq!(super::shell_word("it's"), "'it''s'");
        }
    }
}
