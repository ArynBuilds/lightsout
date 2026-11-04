//! Lights Out terminal: the same app on Windows and in Lights Out.
//! Tabs with split panes, agent status read from the screen, alerts, a
//! control API for agents, sessions that survive restarts and switching.
//! Everything is reachable from one key: Ctrl+Shift+P.

mod api;
mod config;
mod headless;
mod keys;
mod layout;
mod platform;
mod render;
mod session;
mod status;
mod tab;

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
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
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

const PALETTE: [u32; 16] = [
    0x1c1c1c, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd6d6d6,
    0x5c6370, 0xff7b86, 0xb5e890, 0xffd88a, 0x82c4ff, 0xdd9bf0, 0x7fd6e0, 0xffffff,
];
const BAR: u32 = 0x000000; // full black: OLED pixels stay off
const BAR_ACTIVE: u32 = 0x000000;
const DIMTEXT: u32 = 0x7a7a85;
const ALERT: u32 = 0xff6b5b;
const DONE: u32 = 0x6fcf7f;
const BORDER: u32 = 0x26262c;
const PADX: i32 = 6;
const FRAME: Duration = Duration::from_millis(33); // at most ~30 redraws per second, only when something changed

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

struct TabView {
    layout: Node,
    focus: u64,
}

/// A room has its own set of tabs. The current room's tabs live in App.tabs;
/// the others are kept here.
struct Room {
    name: String,
    tabs: Vec<TabView>,
    active: usize,
}

#[derive(Clone, Copy)]
enum Purpose {
    Command,
    RoomName(usize),
}

#[derive(Clone)]
enum Action {
    NewTab,
    Agent(String),
    RunCommand,
    SplitRight,
    SplitDown,
    ClosePane,
    NextTab,
    PrevTab,
    Jump,
    Editor,
    Copy,
    Paste,
    Remote(String),
    Lights,
    BackToWindows,
    Quit,
    Settings,
    FontBigger,
    FontSmaller,
    ToggleSidebar,
    NewRoom,
    RenameRoom,
    CloseRoom,
    GoRoom(usize),
    ScreenOff,
    Custom(config::KeyCmd),
}

struct Item {
    label: String,
    hint: String,
    action: Action,
}

enum Overlay {
    None,
    Palette { title: String, items: Vec<Item>, filter: String, sel: usize },
    Menu { x: i32, y: i32, items: Vec<Item>, sel: usize },
    Confirm(Confirm),
    Ask { title: String, text: String, purpose: Purpose },
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
    Divider { tab: usize, path: Vec<bool>, side_by_side: bool, area: Rect },
    Select { pane: u64, origin: Rect },
}

#[derive(Clone, Copy)]
enum Hit {
    Tab(usize),
    NewTab,
    Palette,
    Switch,
    Pane(u64),
    Room(usize),
    NewRoom,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
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
    battery_text: String,
    battery_at: Instant,
    bar_texts: Vec<String>,
    clipboard: Option<arboard::Clipboard>,
    switching: Option<mpsc::Receiver<Result<(), String>>>,
    waiters: Vec<Waiter>,
    status_at: Instant,
    saved_at: Instant,
    window_focused: bool,
    hidden: bool,
    sidebar: bool,
    running_at: Instant,
    rooms: Vec<Room>,
    room: usize,
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
    std::fs::read_to_string(api::state_dir().join("ui.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
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
    fn new(proxy: EventLoopProxy<UserEvent>, session: String) -> Self {
        let loaded = config::load();
        let theme = theme_from(&loaded.config);
        let mut app = App {
            proxy,
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
            battery_text: String::new(),
            battery_at: Instant::now() - Duration::from_secs(60),
            bar_texts: Vec::new(),
            clipboard: arboard::Clipboard::new().ok(),
            switching: None,
            waiters: Vec::new(),
            status_at: Instant::now(),
            saved_at: Instant::now(),
            window_focused: true,
            hidden: false,
            sidebar: load_ui().sidebar,
            running_at: Instant::now() - Duration::from_secs(10),
            rooms: vec![Room { name: "Room 1".into(), tabs: Vec::new(), active: 0 }],
            room: 0,
        };
        if let Some(e) = loaded.error {
            app.toast(e, ALERT);
        }
        app
    }

    // ------------------------------------------------------------ geometry

    fn bar_h(&self) -> i32 {
        self.fonts.cell_h as i32 + 10
    }

    fn content(&self) -> Rect {
        let bh = self.bar_h();
        let sw = self.sidebar_w();
        Rect { x: sw, y: bh, w: self.size.0 as i32 - sw, h: self.size.1 as i32 - bh }
    }

    /// The agent list on the left (Ctrl+Shift+B), about 26 characters wide.
    fn sidebar_w(&self) -> i32 {
        if self.sidebar && self.size.0 as i32 > 60 * self.fonts.cell_w as i32 {
            26 * self.fonts.cell_w as i32
        } else {
            0
        }
    }

    fn popup_rect(&self) -> Rect {
        let c = self.content();
        let (w, h) = (c.w * 8 / 10, c.h * 8 / 10);
        Rect { x: c.x + (c.w - w) / 2, y: c.y + (c.h - h) / 2, w, h }
    }

    fn grid_size(&self, r: Rect) -> Size {
        let cols = ((r.w - 2 * PADX).max(0) as u32 / self.fonts.cell_w).max(2) as usize;
        let rows = ((r.h - 4).max(0) as u32 / self.fonts.cell_h).max(2) as usize;
        Size { cols, rows }
    }

    fn cell(&self) -> (u32, u32) {
        (self.fonts.cell_w, self.fonts.cell_h)
    }

    fn active_rects(&self) -> Vec<(u64, Rect)> {
        self.tabs.get(self.active).map(|t| t.layout.layout(self.content()).0).unwrap_or_default()
    }

    fn resize_all(&mut self) {
        let content = self.content();
        let cell = self.cell();
        let mut sizes = Vec::new();
        for t in self.tabs.iter().chain(self.rooms.iter().flat_map(|r| r.tabs.iter())) {
            for (id, r) in t.layout.layout(content).0 {
                sizes.push((id, self.grid_size(r)));
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
    }

    // ------------------------------------------------------------ panes and tabs

    fn spawn(&mut self, cwd: PathBuf, command: Option<String>, env: Vec<(String, String)>, size: Size) -> Option<u64> {
        let cwd = if cwd.is_dir() { cwd } else { platform::home() };
        if platform::in_lightsout() {
            let _ = std::process::Command::new("sudo").args(["-n", "/usr/local/sbin/lightsout-split"]).arg(&cwd).status();
        }
        let (program, args) = match &command {
            Some(c) => platform::agent(c),
            None => platform::shell(),
        };
        let id = self.next_id;
        self.next_id += 1;
        match Pane::spawn(id, &Waker::Window(self.proxy.clone()), &self.session, cwd, program, args, command, size, self.cell(), env) {
            Ok(p) => {
                self.panes.insert(id, p);
                self.dirty = true;
                Some(id)
            }
            Err(e) => {
                self.toast(format!("Could not start: {e}"), ALERT);
                None
            }
        }
    }

    fn new_tab(&mut self, cwd: PathBuf, command: Option<String>, env: Vec<(String, String)>) -> Option<u64> {
        let size = self.grid_size(self.content());
        let id = self.spawn(cwd, command, env, size)?;
        self.tabs.push(TabView { layout: Node::Leaf(id), focus: id });
        self.active = self.tabs.len() - 1;
        Some(id)
    }

    fn split(&mut self, side_by_side: bool, command: Option<String>, target: Option<u64>) -> Option<u64> {
        let target = target.or_else(|| self.tabs.get(self.active).map(|t| t.focus))?;
        let ti = self.tab_of(target)?;
        let cwd = self.panes.get(&target).map(|p| p.cwd.clone()).unwrap_or_else(platform::home);
        let size = self.grid_size(self.content());
        let id = self.spawn(cwd, command, Vec::new(), size)?;
        self.tabs[ti].layout.split(target, id, side_by_side);
        self.tabs[ti].focus = id;
        self.active = ti;
        self.resize_all();
        Some(id)
    }

    fn tab_of(&self, pane: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.layout.leaves().contains(&pane))
    }

    fn focused(&self) -> Option<u64> {
        self.popup.or_else(|| self.tabs.get(self.active).map(|t| t.focus))
    }

    fn cwd(&self) -> PathBuf {
        self.focused().and_then(|id| self.panes.get(&id)).map(|p| p.cwd.clone()).unwrap_or_else(platform::home)
    }

    /// Which room a pane is in.
    fn room_of(&self, pane: u64) -> Option<usize> {
        if self.tab_of(pane).is_some() {
            return Some(self.room);
        }
        self.rooms.iter().position(|r| r.tabs.iter().any(|t| t.layout.leaves().contains(&pane)))
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
        if let Some(f) = self.tabs.get(self.active).map(|t| t.focus) {
            if let Some(p) = self.panes.get_mut(&f) {
                p.det.seen();
            }
        }
        self.dirty = true;
    }

    fn new_room(&mut self) {
        let cwd = self.cwd();
        let n = self.rooms.len() + 1;
        self.rooms.push(Room { name: format!("Room {n}"), tabs: Vec::new(), active: 0 });
        self.switch_room(self.rooms.len() - 1);
        self.new_tab(cwd, None, Vec::new());
    }

    /// The current room is empty: drop it and go to a neighbour (or start fresh).
    fn leave_empty_room(&mut self, el: &ActiveEventLoop) {
        if self.rooms.len() > 1 {
            let gone = self.room;
            self.rooms.remove(gone);
            let next = gone.min(self.rooms.len() - 1);
            self.tabs = std::mem::take(&mut self.rooms[next].tabs);
            self.active = self.rooms[next].active.min(self.tabs.len().saturating_sub(1));
            self.room = next;
        } else if platform::in_lightsout() {
            self.new_tab(platform::home(), None, Vec::new());
        } else {
            // the last tab closed: start fresh next time
            let _ = std::fs::remove_file(session::local_file(&self.session));
            el.exit();
        }
    }

    fn close_room(&mut self, el: &ActiveEventLoop) {
        let ids: Vec<u64> = self.tabs.iter().flat_map(|t| t.layout.leaves()).collect();
        for id in ids {
            if let Some(p) = self.panes.remove(&id) {
                p.shutdown();
            }
        }
        self.tabs.clear();
        self.leave_empty_room(el);
        self.resize_all();
    }

    fn focus(&mut self, pane: u64) {
        if let Some(r) = self.room_of(pane) {
            self.switch_room(r);
        }
        if let Some(ti) = self.tab_of(pane) {
            self.active = ti;
            self.tabs[ti].focus = pane;
        }
        if let Some(p) = self.panes.get_mut(&pane) {
            p.det.seen();
        }
        self.dirty = true;
    }

    fn close_pane(&mut self, id: u64, el: &ActiveEventLoop) {
        // a pane in another room closes there, without switching rooms
        if self.popup != Some(id) {
            if let Some(r) = self.room_of(id).filter(|r| *r != self.room) {
                let tabs = &mut self.rooms[r].tabs;
                if let Some(ti) = tabs.iter().position(|t| t.layout.leaves().contains(&id)) {
                    if matches!(tabs[ti].layout, Node::Leaf(_)) {
                        tabs.remove(ti);
                    } else {
                        tabs[ti].layout.remove(id);
                        if tabs[ti].focus == id {
                            tabs[ti].focus = tabs[ti].layout.leaves()[0];
                        }
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
            if matches!(self.tabs[ti].layout, Node::Leaf(_)) {
                self.tabs.remove(ti);
                self.active = self.active.min(self.tabs.len().saturating_sub(1));
            } else {
                self.tabs[ti].layout.remove(id);
                if self.tabs[ti].focus == id {
                    self.tabs[ti].focus = self.tabs[ti].layout.leaves()[0];
                }
            }
        }
        if let Some(p) = self.panes.remove(&id) {
            p.shutdown();
        }
        if self.tabs.is_empty() {
            self.leave_empty_room(el);
        }
        self.resize_all();
    }

    fn toast(&mut self, text: String, color: u32) {
        self.toasts.push(Toast { text, color, at: Instant::now() });
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
        let focused = if self.window_focused && !self.hidden && !self.screen_off { self.focused() } else { None };
        let extra = self.cfg.status.needs_you.clone();
        let watching = !self.waiters.is_empty();
        let mut changes = Vec::new();
        for (id, p) in self.panes.iter_mut() {
            if !watching && !p.det.recently_active() && p.det.state == State::Idle {
                continue;
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
            for p in self.panes.values_mut() {
                let now = p.shell_pid.and_then(|pid| running.get(&pid).cloned().flatten());
                if now != p.running {
                    p.running = now;
                    self.dirty = true;
                }
            }
        }
        for (id, s) in changes {
            self.dirty = true;
            if matches!(s, State::NeedsYou | State::Done) && focused != Some(id) {
                self.alert(id, s);
            }
        }
        self.check_waiters();
    }

    fn alert(&mut self, id: u64, s: State) {
        let name = self.panes.get(&id).map(|p| p.title.clone()).unwrap_or_default();
        let text = match s {
            State::NeedsYou => format!("{name} needs you  (Ctrl+Shift+J)"),
            _ => format!("{name} is done  (Ctrl+Shift+J)"),
        };
        if self.cfg.notify.popup {
            self.toast(text.clone(), if s == State::NeedsYou { ALERT } else { DONE });
            if self.hidden || !self.window_focused {
                platform::os_notify(&text);
            }
        }
        if self.cfg.notify.sound && !platform::in_lightsout() {
            let file = if s == State::NeedsYou { &self.cfg.notify.sound_needs_you } else { &self.cfg.notify.sound_done };
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
        let pids: Vec<u32> = self.panes.values().filter_map(|p| p.shell_pid).collect();
        let running = platform::running_in_tabs(&pids);
        let rooms = self
            .rooms
            .iter()
            .enumerate()
            .map(|(ri, r)| {
                let (tabs, active) = if ri == self.room { (&self.tabs, self.active) } else { (&r.tabs, r.active) };
                session::SavedRoom { name: r.name.clone(), tabs: self.saved_tabs(tabs, &running, screen_lines), active }
            })
            .collect();
        Saved { tabs: Vec::new(), active: 0, rooms, room: self.room }
    }

    fn saved_tabs(&self, tabs: &[TabView], running: &HashMap<u32, Option<String>>, screen_lines: usize) -> Vec<SavedTab> {
        tabs
            .iter()
            .map(|t| {
                let ids = t.layout.leaves();
                let index = |id: u64| ids.iter().position(|x| *x == id).unwrap_or(0) as u64;
                let panes = ids
                    .iter()
                    .filter_map(|id| self.panes.get(id))
                    .map(|p| SavedPane {
                        cwd: p.cwd.to_string_lossy().to_string(),
                        command: p.shell_pid.and_then(|pid| running.get(&pid).cloned().flatten()).or_else(|| p.command.clone()),
                        screen: (screen_lines > 0).then(|| p.text(screen_lines)),
                    })
                    .collect();
                SavedTab { layout: t.layout.remap(&index), panes, focus: index(t.focus) as usize }
            })
            .collect()
    }

    fn save_session(&mut self) {
        self.saved_at = Instant::now();
        if !self.cfg.history.keep || platform::in_lightsout() || (self.tabs.is_empty() && self.rooms.iter().all(|r| r.tabs.is_empty())) {
            return;
        }
        let s = self.saved(200);
        session::save(&session::local_file(&self.session), &s);
    }

    fn restore(&mut self, saved: Saved, from_windows: bool) {
        let mut rooms = saved.rooms;
        if rooms.is_empty() && !saved.tabs.is_empty() {
            rooms.push(session::SavedRoom { name: "Room 1".into(), tabs: saved.tabs, active: saved.active });
        }
        if rooms.is_empty() {
            return;
        }
        self.rooms.clear();
        self.tabs.clear();
        for (ri, r) in rooms.iter().enumerate() {
            self.restore_tabs(&r.tabs, from_windows, ri);
            let tabs = std::mem::take(&mut self.tabs);
            if !tabs.is_empty() {
                self.rooms.push(Room { name: r.name.clone(), active: r.active.min(tabs.len() - 1), tabs });
            }
        }
        if self.rooms.is_empty() {
            self.rooms.push(Room { name: "Room 1".into(), tabs: Vec::new(), active: 0 });
        }
        self.room = saved.room.min(self.rooms.len() - 1);
        self.tabs = std::mem::take(&mut self.rooms[self.room].tabs);
        self.active = self.rooms[self.room].active.min(self.tabs.len().saturating_sub(1));
        self.resize_all();
    }

    fn restore_tabs(&mut self, saved_tabs: &[SavedTab], from_windows: bool, room: usize) {
        for (ti, t) in saved_tabs.iter().enumerate() {
            let mut ids = Vec::new();
            for (n, p) in t.panes.iter().enumerate() {
                let cwd = if from_windows { platform::map_path(&p.cwd) } else { PathBuf::from(&p.cwd) };
                if from_windows {
                    platform::link_claude_history(&p.cwd, &cwd);
                }
                let mut env = Vec::new();
                if let Some(screen) = p.screen.as_ref().filter(|s| !s.trim().is_empty()) {
                    let f = std::env::temp_dir().join(format!("lightsout-screen-{room}-{ti}-{n}.txt"));
                    if std::fs::write(&f, screen).is_ok() {
                        env.push(("LIGHTSOUT_SCREEN".to_string(), f.to_string_lossy().to_string()));
                    }
                }
                // known way to continue (resume.conf) -> run it; otherwise type the command back in
                let resume = p.command.as_deref().and_then(platform::resume_command);
                let typed = if resume.is_none() { p.command.clone() } else { None };
                let size = self.grid_size(self.content());
                let id = self.spawn(cwd, resume, env, size);
                if let (Some(id), Some(cmd)) = (id, typed) {
                    if let Some(pane) = self.panes.get(&id) {
                        pane.write_str(cmd);
                    }
                }
                ids.push(id);
            }
            let Some(first) = ids.iter().flatten().next().copied() else { continue };
            let map = |i: u64| ids.get(i as usize).copied().flatten().unwrap_or(first);
            let layout = t.layout.remap(&map);
            let focus = ids.get(t.focus).copied().flatten().unwrap_or(first);
            self.tabs.push(TabView { layout, focus });
        }
    }

    fn start(&mut self) {
        if platform::in_lightsout() {
            if let Some(s) = platform::take_handoff() {
                self.restore(s, platform::image());
            }
        } else if let Some(s) = platform::take_handoff().filter(|_| cfg!(unix)) {
            // back on the Linux desktop / logged back in on the Mac: the tabs come along
            self.restore(s, false);
        } else if let Some(s) = platform::take_handback() {
            // back in Windows: the tabs from Lights Out come along (conversations continue)
            self.restore(s, true);
        } else if self.cfg.history.keep {
            if let Some(s) = session::load(&session::local_file(&self.session)) {
                self.restore(s, false);
            }
        }
        if platform::drive_read_only() {
            self.toast("C: is read-only: Windows was hibernated. Shut Windows down fully (or restart) before Lights Out to save files.".into(), ALERT);
        }
        if self.tabs.is_empty() {
            let start = std::env::current_dir().ok().filter(|d| d.is_dir()).unwrap_or_else(platform::home);
            self.new_tab(start, None, Vec::new());
        }
        self.start_bar_items();
    }

    fn start_bar_items(&mut self) {
        self.bar_texts = self.cfg.bar.items.iter().map(|i| i.text.clone().unwrap_or_default()).collect();
        for (n, item) in self.cfg.bar.items.iter().enumerate() {
            let Some(cmd) = item.command.clone() else { continue };
            let (proxy, every) = (self.proxy.clone(), item.every.max(5));
            std::thread::spawn(move || loop {
                let (sh, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
                let out = std::process::Command::new(sh).args([flag, &cmd]).output();
                let text = out.map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").chars().take(40).collect()).unwrap_or_default();
                if proxy.send_event(UserEvent::BarText(n, text)).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_secs(every));
            });
        }
    }

    // ------------------------------------------------------------ switching

    fn start_switch(&mut self) {
        let saved = self.saved(80);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(platform::enter_lightsout(&saved));
        });
        self.switching = Some(rx);
        self.toast("Switching to Lights Out... saving your tabs".into(), self.theme.accent);
    }

    // ------------------------------------------------------------ actions

    fn actions(&self) -> Vec<Item> {
        let lo = platform::in_lightsout();
        let item = |label: &str, hint: &str, action: Action| Item { label: label.into(), hint: hint.into(), action };
        let mut v = vec![
            item("New tab", "Ctrl+Shift+T", Action::NewTab),
            item("Start an agent...", "Ctrl+Shift+A", Action::RunCommand),
            item("Split right", "Ctrl+Shift+Right", Action::SplitRight),
            item("Split down", "Ctrl+Shift+Down", Action::SplitDown),
            item("Move between panes", "Alt+arrows", Action::NextTab),
            item("Close pane", "Ctrl+Shift+W", Action::ClosePane),
            item("Jump to the agent that needs you", "Ctrl+Shift+J", Action::Jump),
            item("Next tab", "Ctrl+Tab", Action::NextTab),
            item("Previous tab", "Ctrl+Shift+Tab", Action::PrevTab),
            item("Open output in an editor", "Ctrl+Shift+E", Action::Editor),
            item("Copy selection", "Ctrl+Shift+C", Action::Copy),
            item("Paste", "Ctrl+Shift+V", Action::Paste),
            item("Bigger text", "Ctrl+=", Action::FontBigger),
            item("Smaller text", "Ctrl+-", Action::FontSmaller),
            item("New room", "Ctrl+Shift+N", Action::NewRoom),
            item("Rename this room", "", Action::RenameRoom),
            item("Close this room (stops its agents)", "", Action::CloseRoom),
            item("Show / hide the sidebar", "Ctrl+Shift+B", Action::ToggleSidebar),
            item("Settings", "Ctrl+,", Action::Settings),
        ];
        if lo {
            v.push(item("Screen off (any key turns it on)", "Ctrl+Shift+O", Action::ScreenOff));
            let back = if platform::image() { "Lights On (back to Windows)" } else { "Lights On (back to the desktop)" };
            v.push(item(back, "Ctrl+Shift+L", Action::BackToWindows));
        } else {
            let what = if cfg!(windows) { "Lights Out (Windows off, only this terminal)" } else { "Lights Out (desktop off, only this terminal)" };
            v.push(item(what, "Ctrl+Shift+L", Action::Lights));
            v.push(item("Quit (stops all agents)", "Ctrl+Shift+Q", Action::Quit));
        }
        for (i, r) in self.rooms.iter().enumerate() {
            if i != self.room {
                v.push(item(&format!("Go to {}", r.name), "room", Action::GoRoom(i)));
            }
        }
        for m in &self.cfg.machine {
            v.push(item(&format!("Connect to {}", m.name), &m.ssh, Action::Remote(m.ssh.clone())));
        }
        for (k, c) in &self.cfg.keys {
            v.push(item(&format!("Run: {}", c.command), k, Action::Custom(c.clone())));
        }
        v
    }

    fn agent_items(&self) -> Vec<Item> {
        // the programs listed in resume.conf (data), plus "any command"
        let mut names: Vec<String> = Vec::new();
        for f in [PathBuf::from("/usr/local/share/lightsout/resume.conf"), platform::home().join(".config/lightsout/resume.conf")] {
            for line in std::fs::read_to_string(f).unwrap_or_default().lines() {
                let line = line.trim();
                if line.starts_with('#') {
                    continue;
                }
                if let Some((p, _)) = line.split_once('=') {
                    let p = p.trim().to_string();
                    if !p.is_empty() && !names.contains(&p) {
                        names.push(p);
                    }
                }
            }
        }
        if names.is_empty() {
            names = vec!["claude".into(), "codex".into()];
        }
        let mut v: Vec<Item> = names.into_iter().map(|n| Item { label: n.clone(), hint: "new tab".into(), action: Action::Agent(n) }).collect();
        v.push(Item { label: "Any other command...".into(), hint: "type it".into(), action: Action::RunCommand });
        v
    }

    fn open_palette(&mut self) {
        self.overlay = Overlay::Palette { title: "Everything Lights Out can do".into(), items: self.actions(), filter: String::new(), sel: 0 };
        self.dirty = true;
    }

    fn run(&mut self, a: Action, el: &ActiveEventLoop) {
        let cwd = self.cwd();
        match a {
            Action::NewTab => {
                self.new_tab(cwd, None, Vec::new());
            }
            Action::Agent(c) => {
                self.new_tab(cwd, Some(c), Vec::new());
            }
            Action::RunCommand => {
                self.overlay = Overlay::Ask { title: "Command to run in a new tab:".into(), text: String::new(), purpose: Purpose::Command }
            }
            Action::NewRoom => self.new_room(),
            Action::RenameRoom => {
                let text = self.rooms[self.room].name.clone();
                self.overlay = Overlay::Ask { title: "Room name:".into(), text, purpose: Purpose::RoomName(self.room) };
            }
            Action::CloseRoom => self.close_room(el),
            Action::GoRoom(i) => self.switch_room(i),
            Action::SplitRight => {
                self.split(true, None, None);
            }
            Action::SplitDown => {
                self.split(false, None, None);
            }
            Action::ClosePane => {
                if let Some(id) = self.focused() {
                    self.close_pane(id, el);
                }
            }
            Action::NextTab => self.cycle(1),
            Action::PrevTab => self.cycle(-1),
            Action::Jump => self.jump(),
            Action::Editor => self.open_in_editor(),
            Action::Copy => self.copy(),
            Action::Paste => self.paste(),
            Action::Remote(target) => {
                self.new_tab(cwd, Some(format!("ssh {target}")), Vec::new());
            }
            Action::Lights => {
                if platform::in_lightsout() {
                    self.overlay = Overlay::Confirm(Confirm::BackToWindows);
                } else {
                    self.overlay = Overlay::Confirm(Confirm::EnterLightsOut);
                }
            }
            Action::ScreenOff => {
                platform::screen(false);
                // macOS wakes the display itself (lid, trackpad), so keep drawing there
                self.screen_off = !cfg!(target_os = "macos");
            }
            Action::BackToWindows => self.overlay = Overlay::Confirm(Confirm::BackToWindows),
            Action::Quit => self.overlay = Overlay::Confirm(Confirm::Quit),
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
                save_ui(&UiState { sidebar: self.sidebar });
                self.resize_all();
            }
            Action::Custom(k) => match k.mode.as_str() {
                "tab" => {
                    self.new_tab(cwd, Some(k.command), Vec::new());
                }
                "split" => {
                    self.split(true, Some(k.command), None);
                }
                _ => {
                    let size = self.grid_size(self.popup_rect());
                    // the popup closes itself when the command ends
                    let cmd = format!("{}; exit", k.command);
                    if let Some(id) = self.spawn(cwd, Some(cmd), Vec::new(), size) {
                        self.popup = Some(id);
                    }
                }
            },
        }
        self.dirty = true;
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
        let text = self.focused().and_then(|id| self.panes.get(&id)).and_then(|p| p.term.lock().selection_to_string());
        if let (Some(t), Some(c)) = (text, self.clipboard.as_mut()) {
            let _ = c.set_text(t);
            self.toast("Copied".into(), DIMTEXT);
        }
    }

    fn paste(&mut self) {
        let Some(text) = self.clipboard.as_mut().and_then(|c| c.get_text().ok()) else { return };
        let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) else { return };
        let bracketed = p.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        p.input(if bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text }.into_bytes());
    }

    fn open_in_editor(&mut self) {
        let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) else { return };
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

    fn shortcut(&mut self, key: &Key, el: &ActiveEventLoop) -> bool {
        // your own keys from the settings file come first
        if let Some(name) = self.key_name(key) {
            if let Some(k) = self.cfg.keys.get(&name).cloned() {
                self.run(Action::Custom(k), el);
                return true;
            }
        }
        let m = self.mods;
        let ch = match key {
            Key::Character(s) => s.chars().next().map(|c| c.to_ascii_lowercase()),
            _ => None,
        };
        let action = if m.control_key() && m.shift_key() {
            match (ch, key) {
                (Some('p'), _) => {
                    self.open_palette();
                    return true;
                }
                (Some('a'), _) => {
                    self.overlay = Overlay::Palette { title: "Start an agent in a new tab".into(), items: self.agent_items(), filter: String::new(), sel: 0 };
                    self.dirty = true;
                    return true;
                }
                (Some('t'), _) => Action::NewTab,
                (Some('b'), _) => Action::ToggleSidebar,
                (Some('n'), _) => Action::NewRoom,
                (Some('w'), _) => Action::ClosePane,
                (Some('j'), _) => Action::Jump,
                (Some('e'), _) => Action::Editor,
                (Some('c'), _) => Action::Copy,
                (Some('v'), _) => Action::Paste,
                (Some('l'), _) => Action::Lights,
                (Some('o'), _) => Action::ScreenOff,
                (Some('q'), _) => {
                    if platform::in_lightsout() {
                        Action::BackToWindows
                    } else {
                        Action::Quit
                    }
                }
                (_, Key::Named(NamedKey::ArrowRight)) => Action::SplitRight,
                (_, Key::Named(NamedKey::ArrowDown)) => Action::SplitDown,
                (_, Key::Named(NamedKey::Tab)) => Action::PrevTab,
                _ => return false,
            }
        } else if m.control_key() && !m.alt_key() {
            match (ch, key) {
                (_, Key::Named(NamedKey::Tab)) => Action::NextTab,
                (Some('='), _) | (Some('+'), _) => Action::FontBigger,
                (Some('-'), _) => Action::FontSmaller,
                (Some(','), _) => Action::Settings,
                _ => return false,
            }
        } else if m.alt_key() && !m.control_key() {
            let dir = match key {
                Key::Named(NamedKey::ArrowLeft) => Some((-1, 0)),
                Key::Named(NamedKey::ArrowRight) => Some((1, 0)),
                Key::Named(NamedKey::ArrowUp) => Some((0, -1)),
                Key::Named(NamedKey::ArrowDown) => Some((0, 1)),
                _ => None,
            };
            if let Some((dx, dy)) = dir {
                if let Some(from) = self.tabs.get(self.active).map(|t| t.focus) {
                    if let Some(to) = layout::neighbor(&self.active_rects(), from, dx, dy) {
                        self.focus(to);
                    }
                }
                return true;
            }
            if let Some(d @ '1'..='9') = ch {
                let i = d as usize - '1' as usize;
                if i < self.tabs.len() {
                    let f = self.tabs[i].focus;
                    self.focus(f);
                }
                return true;
            }
            return false;
        } else if m.shift_key() {
            let rows = self.focused().and_then(|id| self.panes.get(&id)).map(|p| p.size.rows as i32).unwrap_or(20);
            match key {
                Key::Named(NamedKey::PageUp) => self.scroll(rows / 2),
                Key::Named(NamedKey::PageDown) => self.scroll(-rows / 2),
                Key::Named(NamedKey::Insert) => self.paste(),
                _ => return false,
            }
            return true;
        } else {
            return false;
        };
        self.run(action, el);
        true
    }

    fn scroll(&mut self, lines: i32) {
        if let Some(p) = self.focused().and_then(|id| self.panes.get(&id)) {
            p.scroll(lines);
            self.dirty = true;
        }
    }

    /// Keys while a palette, menu, question or confirmation is open.
    fn overlay_key(&mut self, key: &Key, text: Option<&str>, el: &ActiveEventLoop) -> bool {
        if matches!(self.overlay, Overlay::None) {
            return false;
        }
        self.dirty = true;
        if let Overlay::Confirm(c) = self.overlay {
            self.overlay = Overlay::None;
            if matches!(key, Key::Named(NamedKey::Enter)) {
                match c {
                    Confirm::BackToWindows => {
                        platform::back_to_windows(&self.saved(80));
                        if cfg!(target_os = "macos") {
                            if let Some(w) = &self.window {
                                w.set_fullscreen(None);
                            }
                        }
                    }
                    Confirm::EnterLightsOut => self.start_switch(),
                    Confirm::Quit => {
                        self.save_session();
                        for p in self.panes.values() {
                            p.shutdown();
                        }
                        el.exit();
                    }
                }
            }
            return true;
        }
        if let Overlay::Ask { text: t, purpose, .. } = &mut self.overlay {
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
                                if let Some(r) = self.rooms.get_mut(i) {
                                    r.name = value;
                                }
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
                let action = visible.get(self.sel()).cloned();
                self.overlay = Overlay::None;
                if let Some(a) = action {
                    self.run(a, el);
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if let Overlay::Palette { filter, sel, .. } = &mut self.overlay {
                    filter.pop();
                    *sel = 0;
                }
            }
            _ => {
                if let (Some(s), Overlay::Palette { filter, sel, .. }) = (text, &mut self.overlay) {
                    if !s.chars().any(char::is_control) {
                        filter.push_str(s);
                        *sel = 0;
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
                items.iter().filter(|i| f.is_empty() || i.label.to_lowercase().contains(&f)).map(|i| i.action.clone()).collect()
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
        if let Overlay::Palette { sel, .. } | Overlay::Menu { sel, .. } = &mut self.overlay {
            if n > 0 {
                *sel = ((*sel as i32 + d).rem_euclid(n as i32)) as usize;
            }
        }
    }

    fn lock_key(&mut self, key: &Key) {
        let Lock::Locked { salt, hash, wrong } = &mut self.lock else { return };
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
        self.active_rects().into_iter().find(|(_, r)| r.contains(x, y))
    }

    fn cell_point(&self, pane: u64, r: Rect, x: i32, y: i32) -> Option<(Point, Side)> {
        let p = self.panes.get(&pane)?;
        let (cw, ch) = (self.fonts.cell_w as i32, self.fonts.cell_h as i32);
        let col = ((x - r.x - PADX).max(0) / cw).min(p.size.cols as i32 - 1);
        let row = ((y - r.y - 2).max(0) / ch).min(p.size.rows as i32 - 1);
        let offset = p.term.lock().grid().display_offset() as i32;
        let side = if (x - r.x - PADX).rem_euclid(cw) > cw / 2 { Side::Right } else { Side::Left };
        Some((Point::new(Line(row - offset), Column(col as usize)), side))
    }

    fn mouse_down(&mut self, button: MouseButton, el: &ActiveEventLoop) {
        let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
        self.dirty = true;
        if let Overlay::Menu { x: mx, y: my, items, .. } = &self.overlay {
            // a click on a menu row runs it; anywhere else closes the menu
            let cw = self.fonts.cell_w as i32;
            let ch = self.fonts.cell_h as i32;
            let mw = items.iter().map(|i| i.label.chars().count() + i.hint.chars().count() + 4).max().unwrap_or(10) as i32 * cw;
            let row = (y - *my - 4) / ch;
            let action = (x >= *mx && x < *mx + mw && y >= *my + 4 && row >= 0).then(|| items.get(row as usize).map(|i| i.action.clone())).flatten();
            self.overlay = Overlay::None;
            if let Some(a) = action {
                self.run(a, el);
            }
            return;
        }
        if !matches!(self.overlay, Overlay::None) {
            return;
        }
        if x < self.sidebar_w() {
            let hit = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|h| h.1);
            match hit {
                Some(Hit::Pane(id)) => self.focus(id),
                Some(Hit::Room(i)) => self.switch_room(i),
                Some(Hit::NewRoom) => self.new_room(),
                Some(Hit::Palette) => self.open_palette(),
                Some(Hit::Switch) => {
                    self.overlay = Overlay::Confirm(if platform::in_lightsout() { Confirm::BackToWindows } else { Confirm::EnterLightsOut });
                }
                _ => {}
            }
            return;
        }
        if y < self.bar_h() {
            let hit = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|h| h.1);
            match hit {
                Some(Hit::Tab(i)) if i < self.tabs.len() => {
                    let f = self.tabs[i].focus;
                    self.focus(f);
                }
                Some(Hit::NewTab) => {
                    let cwd = self.cwd();
                    self.new_tab(cwd, None, Vec::new());
                }
                Some(Hit::Palette) => self.open_palette(),
                Some(Hit::Switch) => {
                    self.overlay = Overlay::Confirm(if platform::in_lightsout() { Confirm::BackToWindows } else { Confirm::EnterLightsOut });
                }
                _ => {}
            }
            return;
        }
        // dragging the line between two panes?
        if button == MouseButton::Left && self.popup.is_none() {
            if let Some(t) = self.tabs.get(self.active) {
                for d in t.layout.layout(self.content()).1 {
                    let near = if d.side_by_side {
                        (x - d.at).abs() <= 4 && d.area.contains(d.at, y)
                    } else {
                        (y - d.at).abs() <= 4 && d.area.contains(x, d.at)
                    };
                    if near {
                        self.drag = Some(Drag::Divider { tab: self.active, path: d.path, side_by_side: d.side_by_side, area: d.area });
                        return;
                    }
                }
            }
        }
        let Some((pane, r)) = self.pane_at(x, y) else { return };
        if self.popup != Some(pane) {
            self.focus(pane);
        }
        match button {
            MouseButton::Left => {
                if let Some((pt, side)) = self.cell_point(pane, r, x, y) {
                    if let Some(p) = self.panes.get(&pane) {
                        p.term.lock().selection = Some(Selection::new(SelectionType::Simple, pt, side));
                    }
                    self.drag = Some(Drag::Select { pane, origin: r });
                }
            }
            MouseButton::Right => {
                let item = |l: &str, h: &str, a: Action| Item { label: l.into(), hint: h.into(), action: a };
                let items = vec![
                    item("Copy", "Ctrl+Shift+C", Action::Copy),
                    item("Paste", "Ctrl+Shift+V", Action::Paste),
                    item("Split right", "Ctrl+Shift+Right", Action::SplitRight),
                    item("Split down", "Ctrl+Shift+Down", Action::SplitDown),
                    item("Open output in an editor", "Ctrl+Shift+E", Action::Editor),
                    item("Close pane", "Ctrl+Shift+W", Action::ClosePane),
                ];
                self.overlay = Overlay::Menu { x, y, items, sel: 0 };
            }
            _ => {}
        }
    }

    fn mouse_move(&mut self) {
        let (x, y) = (self.mouse.x as i32, self.mouse.y as i32);
        match &self.drag {
            Some(Drag::Divider { tab, path, side_by_side, area }) => {
                let ratio = if *side_by_side { (x - area.x) as f32 / area.w.max(1) as f32 } else { (y - area.y) as f32 / area.h.max(1) as f32 };
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
            None => {}
        }
    }

    fn mouse_up(&mut self) {
        match self.drag.take() {
            Some(Drag::Select { pane, .. }) => {
                let text = self.panes.get(&pane).and_then(|p| p.term.lock().selection_to_string()).filter(|t| !t.is_empty());
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
            Some(Drag::Divider { .. }) | None => {}
        }
    }

    // ------------------------------------------------------------ API (`lightsout <command>`)

    fn api(&mut self, req: api::Request, el: &ActiveEventLoop) {
        let a = req.args.clone();
        let pane_arg = |a: &Value| -> Option<u64> { a["pane"].as_u64().or_else(|| a["pane"].as_str().and_then(|s| s.parse().ok())) };
        let resp = match req.cmd.as_str() {
            "show" => {
                if let Some(w) = &self.window {
                    w.set_visible(true);
                    w.focus_window();
                }
                self.hidden = false;
                json!({ "ok": true })
            }
            "list" => {
                let mut v = Vec::new();
                for (ri, room) in self.rooms.iter().enumerate() {
                    let tabs = if ri == self.room { &self.tabs } else { &room.tabs };
                    for (ti, t) in tabs.iter().enumerate() {
                        for id in t.layout.leaves() {
                            if let Some(p) = self.panes.get(&id) {
                                v.push(json!({ "id": id, "room": room.name, "tab": ti + 1, "title": p.title, "cwd": p.cwd.to_string_lossy(), "state": p.det.state.name(), "command": p.command, "running": p.running }));
                            }
                        }
                    }
                }
                json!({ "panes": v })
            }
            "new" => {
                let cwd = a["cwd"].as_str().map(PathBuf::from).unwrap_or_else(|| self.cwd());
                let cmd = a["command"].as_str().map(str::to_string).filter(|c| !c.is_empty());
                let id = match a["split"].as_str() {
                    Some("right") => self.split(true, cmd, pane_arg(&a)),
                    Some("down") => self.split(false, cmd, pane_arg(&a)),
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
                Some(p) => json!({ "text": p.text(a["lines"].as_u64().unwrap_or(60) as usize), "state": p.det.state.name() }),
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
                    let deadline = Instant::now() + Duration::from_secs(a["timeout"].as_u64().unwrap_or(600));
                    // give the agent a moment to start before checking
                    if let Some(p) = self.panes.get_mut(&id) {
                        p.det.on_output();
                    }
                    self.waiters.push(Waiter { pane: id, until, deadline, reply: req.reply, since: Instant::now(), seen_busy: false });
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
            "notify" => {
                let t = a["text"].as_str().unwrap_or("").to_string();
                self.toast(t.clone(), self.theme.accent);
                if self.hidden || !self.window_focused {
                    platform::os_notify(&t);
                }
                json!({ "ok": true })
            }
            "screen" => {
                let on = a["on"].as_bool().unwrap_or(true);
                platform::screen(on);
                self.screen_off = !on;
                json!({ "ok": true })
            }
            "power" => match platform::battery() {
                Some(b) => json!({ "percent": b.percent, "watts": b.watts, "charging": b.charging }),
                None => json!({ "battery": null }),
            },
            "switch" => {
                if platform::in_lightsout() {
                    platform::back_to_windows(&self.saved(80));
                } else {
                    self.start_switch();
                }
                json!({ "ok": true })
            }
            _ => json!({ "error": format!("unknown command `{}`", req.cmd) }),
        };
        let _ = req.reply.send(resp);
        let _ = el;
        self.dirty = true;
    }

    // ------------------------------------------------------------ drawing

    fn refresh_battery(&mut self) {
        if self.battery_at.elapsed() < Duration::from_secs(20) {
            return;
        }
        self.battery_at = Instant::now();
        self.battery_text = match platform::battery() {
            Some(b) => {
                let w = b.watts.filter(|w| *w > 0.05 && !b.charging).map(|w| format!(" {w:.1} W")).unwrap_or_default();
                format!("{}{}%{}", if b.charging { "+" } else { "" }, b.percent, w)
            }
            None => String::new(),
        };
    }

    fn draw(&mut self) {
        let t = self.theme;
        let (w, h) = self.size;
        let bar = self.bar_h();
        let content = self.content();
        let active_rects = self.active_rects();
        let dividers = self.tabs.get(self.active).map(|tv| tv.layout.layout(content).1).unwrap_or_default();
        let focused = self.focused();
        let tab_states: Vec<(String, State)> = self
            .tabs
            .iter()
            .map(|tv| {
                let leaves = tv.layout.leaves();
                let title = self.panes.get(&tv.focus).map(|p| p.title.clone()).unwrap_or_default();
                let state = leaves.iter().filter_map(|id| self.panes.get(id)).map(|p| p.det.state).max_by_key(|s| urgency(*s)).unwrap_or(State::Idle);
                (if leaves.len() > 1 { format!("{title} +{}", leaves.len() - 1) } else { title }, state)
            })
            .collect();
        let popup = self.popup.map(|p| (p, self.popup_rect()));
        let sel = self.sel();
        let sw = self.sidebar_w();
        let agent_of = |id: u64| -> Option<AgentRow> {
            let p = self.panes.get(&id)?;
            let program = p.running.clone()?;
            let folder = p.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // runtimes like node/python say little; their window title usually says more
            let generic = ["node", "python", "python3", "bun", "deno", "java", "ruby"].contains(&program.as_str());
            let name = if generic && !p.title.is_empty() && p.title != folder { p.title.clone() } else { program };
            Some((id, name, p.det.state, folder))
        };
        let rooms: Vec<(String, Vec<AgentRow>)> = self
            .rooms
            .iter()
            .enumerate()
            .map(|(ri, r)| {
                let tabs = if ri == self.room { &self.tabs } else { &r.tabs };
                (r.name.clone(), tabs.iter().flat_map(|tv| tv.layout.leaves()).filter_map(agent_of).collect())
            })
            .collect();


        let Some(surface) = self.surface.as_mut() else { return };
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else { return };
        if surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else { return };
        let mut cv = Canvas { buf: &mut buffer, w, h };
        cv.fill(t.bg);
        let f = &mut self.fonts;
        let (cw, ch) = (f.cell_w as i32, f.cell_h as i32);
        let center = |s: &str| (w as i32 - s.chars().count() as i32 * cw) / 2;

        if let Lock::Locked { wrong, .. } = &self.lock {
            let cy = h as i32 / 2 - ch * 2;
            let s = "L I G H T S   O U T";
            cv.text(f, center(s), cy, s, true, t.fg);
            let s = if *wrong > 0 { "Wrong PIN. Try again." } else { "Locked. Type your PIN and press Enter." };
            cv.text(f, center(s), cy + ch * 2, s, false, if *wrong > 0 { t.fg } else { DIMTEXT });
            let dots: String = "●".repeat(self.pin.chars().count());
            cv.text(f, center(&dots), cy + ch * 4, &dots, false, t.fg);
            let _ = buffer.present();
            return;
        }

        // ---- top bar: tabs left; your items, battery and buttons right
        cv.rect(sw, 0, w - sw as u32, bar as u32, BAR);
        cv.rect(sw, bar - 1, w - sw as u32, 1, BORDER);
        self.hits.clear();
        let ty = 5;
        let mut x = sw + PADX;
        for (i, (title, state)) in tab_states.iter().enumerate() {
            let title: String = title.chars().take(24).collect();
            let label = format!("  {} {}  ", i + 1, title);
            let wpx = (label.chars().count() as i32 + 1) * cw;
            let (mark, mark_color) = state_mark(*state);
            cv.ch(f, x + cw / 2, ty, mark, false, mark_color);
            cv.text(f, x + cw, ty, &label, i == self.active, if i == self.active { t.fg } else { DIMTEXT });
            self.hits.push((Rect { x, y: 0, w: wpx, h: bar }, Hit::Tab(i)));
            x += wpx + 2;
        }
        cv.text(f, x, ty, " + ", false, DIMTEXT);
        self.hits.push((Rect { x, y: 0, w: 3 * cw, h: bar }, Hit::NewTab));

        let lo = platform::in_lightsout();
        let mut right: Vec<(String, Option<Hit>)> = self.bar_texts.iter().filter(|s| !s.is_empty()).map(|s| (s.clone(), None)).collect();
        if sw == 0 {
            // no sidebar: its battery and buttons come up here
            right.push((self.battery_text.clone(), None));
            right.push((if lo { "Lights On" } else { "Lights Out" }.into(), Some(Hit::Switch)));
            right.push(("actions".into(), Some(Hit::Palette)));
        }
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
            cv.text(f, rx, ty, &s, false, if hit.is_some() { t.fg } else { DIMTEXT });
            if let Some(hh) = hit {
                self.hits.push((Rect { x: rx, y: 0, w: wpx, h: bar }, hh));
            }
            rx -= cw;
        }

        // ---- sidebar: top half = room names; bottom half = what runs in this room, one line each
        if sw > 0 {
            let side = Rect { x: 0, y: 0, w: sw, h: h as i32 };
            cv.rect(side.x + side.w - 1, side.y, 1, side.h as u32, BORDER);
            let max_chars = (sw / cw - 3).max(4) as usize;
            let bottom = side.h - 3 * ch - 12;
            let mid = bottom / 2;
            let row = ch + 6;
            // rooms: names only
            let mut y = 10;
            for (ri, (name, _)) in rooms.iter().enumerate() {
                if y + row > mid {
                    break;
                }
                let current = ri == self.room;
                let label: String = name.chars().take(max_chars).collect();
                cv.text(f, PADX, y, &label, current, if current { t.fg } else { DIMTEXT });
                self.hits.push((Rect { x: 0, y: y - 3, w: sw, h: row }, Hit::Room(ri)));
                y += row;
            }
            if y + row <= mid {
                cv.text(f, PADX, y, "+ room", false, 0x55555c);
                self.hits.push((Rect { x: 0, y: y - 3, w: sw, h: row }, Hit::NewRoom));
            }
            cv.rect(PADX, mid, (sw - 2 * PADX) as u32, 1, BORDER);
            // agents in the current room: mark + name, folder dimmed after it when it fits
            let mut y = mid + 12;
            if let Some((_, agents)) = rooms.get(self.room) {
                for (id, title, state, folder) in agents {
                    if y + row > bottom {
                        break;
                    }
                    let (mark, mark_color) = state_mark(*state);
                    cv.ch(f, PADX, y, mark, false, mark_color);
                    let name: String = title.chars().take(max_chars - 2).collect();
                    cv.text(f, PADX + 2 * cw, y, &name, false, 0xc8c8d0);
                    let used = name.chars().count() + 3;
                    if !folder.is_empty() && folder != title && used + 2 < max_chars {
                        let rest: String = folder.chars().take(max_chars - used).collect();
                        cv.text(f, PADX + (used as i32 + 1) * cw, y, &rest, false, DIMTEXT);
                    }
                    self.hits.push((Rect { x: 0, y: y - 3, w: sw, h: row }, Hit::Pane(*id)));
                    y += row;
                }
            }
            // bottom left
            let by = side.h - 2 * ch - 10;
            cv.text(f, PADX, by, &self.battery_text, false, DIMTEXT);
            // Windows: "Lights Out" switches over; in Lights Out: "Lights On" goes back to Windows
            let label = if lo { "Lights On" } else { "Lights Out" };
            cv.text(f, PADX, by + ch + 2, label, false, t.fg);
            self.hits.push((Rect { x: PADX, y: by + ch, w: label.len() as i32 * cw, h: ch + 4 }, Hit::Switch));
            let ax = PADX + (label.len() as i32 + 3) * cw;
            cv.text(f, ax, by + ch + 2, "actions", false, t.fg);
            self.hits.push((Rect { x: ax, y: by + ch, w: 7 * cw, h: ch + 4 }, Hit::Palette));
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
                let width = if flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
                if bg != t.bg {
                    cv.rect(px, py, (cw * width) as u32, ch as u32, bg);
                }
                cv.ch(f, px, py, cell.c, flags.contains(Flags::BOLD), fg);
                if flags.intersects(Flags::ALL_UNDERLINES) {
                    cv.rect(px, py + ch - 2, (cw * width) as u32, 1, fg);
                }
            }
            let cur = content.cursor;
            if content.mode.contains(TermMode::SHOW_CURSOR) && cur.shape != CursorShape::Hidden && focused == Some(p.id) {
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
        if let Some((id, r)) = popup {
            cv.rect(r.x - 1, r.y - 1, (r.w + 2) as u32, (r.h + 2) as u32, BORDER);
            cv.rect(r.x, r.y, r.w as u32, r.h as u32, t.bg);
            if let Some(p) = self.panes.get(&id) {
                draw_pane(&mut cv, f, p, r);
            }
        }

        // ---- overlays
        match &self.overlay {
            Overlay::Palette { title, items, filter, .. } => {
                let f_l = filter.to_lowercase();
                let shown: Vec<&Item> = items.iter().filter(|i| f_l.is_empty() || i.label.to_lowercase().contains(&f_l)).collect();
                let pw = (w as i32 * 6 / 10).max(40 * cw).min(w as i32 - 20);
                let px = (w as i32 - pw) / 2;
                let rows = shown.len().min(16) as i32;
                let ph = (rows + 3) * ch + 12;
                let py = bar + ch;
                cv.rect(px - 1, py - 1, (pw + 2) as u32, (ph + 2) as u32, BORDER);
                cv.rect(px, py, pw as u32, ph as u32, 0x000000);
                cv.text(f, px + cw, py + 6, title, true, t.fg);
                cv.text(f, px + cw, py + 6 + ch, &format!("› {filter}"), false, t.fg);
                let start = sel.saturating_sub(15);
                for (n, it) in shown.iter().enumerate().skip(start).take(16) {
                    let y = py + 6 + (2 + (n - start) as i32) * ch;
                    if n == sel {
                        cv.rect(px + 2, y, (pw - 4) as u32, ch as u32, BAR_ACTIVE);
                    }
                    cv.text(f, px + cw, y, &it.label, n == sel, if n == sel { t.fg } else { DIMTEXT });
                    let hx = px + pw - (it.hint.chars().count() as i32 + 1) * cw;
                    cv.text(f, hx, y, &it.hint, false, DIMTEXT);
                }
            }
            Overlay::Menu { x: mx, y: my, items, sel } => {
                let mw = items.iter().map(|i| i.label.chars().count() + i.hint.chars().count() + 4).max().unwrap_or(10) as i32 * cw;
                let mh = items.len() as i32 * ch + 8;
                let (mx, my) = ((*mx).min(w as i32 - mw - 2), (*my).min(h as i32 - mh - 2));
                cv.rect(mx - 1, my - 1, (mw + 2) as u32, (mh + 2) as u32, BORDER);
                cv.rect(mx, my, mw as u32, mh as u32, 0x000000);
                for (n, it) in items.iter().enumerate() {
                    let y = my + 4 + n as i32 * ch;
                    if n == *sel {
                        cv.rect(mx + 2, y, (mw - 4) as u32, ch as u32, BAR_ACTIVE);
                    }
                    cv.text(f, mx + cw, y, &it.label, n == *sel, if n == *sel { t.fg } else { 0xa8a8b0 });
                    cv.text(f, mx + mw - (it.hint.chars().count() as i32 + 1) * cw, y, &it.hint, false, DIMTEXT);
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
            Overlay::Ask { title, text, .. } => {
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
            cv.rect(tx - 1, toast_y - 1, (tw + 2) as u32, (ch + 8) as u32, BORDER);
            cv.rect(tx, toast_y, tw as u32, (ch + 6) as u32, 0x000000);
            cv.text(f, tx + cw, toast_y + 3, &toast.text, false, t.fg);
        }
        let _ = buffer.present();
    }

    fn reload_config_if_changed(&mut self) {
        self.cfg_checked = Instant::now();
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
                if self.cfg.theme.font_size > 0.0 && self.cfg.theme.font_size != font_before {
                    let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
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
        let mut attrs = Window::default_attributes().with_title("Lights Out").with_inner_size(LogicalSize::new(1180, 740));
        if platform::in_lightsout() {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = Rc::new(el.create_window(attrs).expect("window"));
        let context = softbuffer::Context::new(window.clone()).expect("display");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
        let size = window.inner_size();
        self.size = (size.width.max(1), size.height.max(1));
        let base = if self.cfg.theme.font_size > 0.0 { self.cfg.theme.font_size } else { 15.0 };
        let mut px = base * window.scale_factor() as f32;
        if platform::in_lightsout() && self.cfg.theme.font_size <= 0.0 {
            px = px.max(self.size.1 as f32 / 58.0); // readable on high-resolution laptop screens
        }
        self.fonts.set_size(px);
        self.window = Some(window);
        self.surface = Some(surface);
        self._context = Some(context);
        if api::serve(&self.session, Waker::Window(self.proxy.clone())).is_err() {
            self.toast("Control commands are unavailable (could not open the local port)".into(), ALERT);
        }
        self.start();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                platform::os_notify("Lights Out keeps your agents running. Type `lightsout` to bring the window back.");
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
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } if self.screen_off => {
                // a click or tap wakes the screen (and does nothing else)
                platform::screen(true);
                self.screen_off = false;
                self.dirty = true;
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => self.mouse_down(button, el),
                ElementState::Released => self.mouse_up(),
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / self.fonts.cell_h as f64) as i32,
                };
                if let Some((id, _)) = self.pane_at(self.mouse.x as i32, self.mouse.y as i32) {
                    if let Some(p) = self.panes.get(&id) {
                        p.scroll(lines);
                        self.dirty = true;
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.screen_off {
                    platform::screen(true);
                    self.screen_off = false;
                    self.dirty = true;
                    return;
                }
                if matches!(self.lock, Lock::Locked { .. }) {
                    self.lock_key(&event.logical_key);
                    return;
                }
                let text = event.text.as_ref().map(|s| s.to_string());
                if self.overlay_key(&event.logical_key, text.as_deref(), el) {
                    return;
                }
                if self.shortcut(&event.logical_key, el) {
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
                    if let Some(bytes) = keys::to_bytes(&event, mods, app_cursor) {
                        p.input(bytes);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.refresh_battery();
                self.draw();
                self.dirty = false;
                self.last_draw = Instant::now();
            }
            _ => {}
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        let (id, ev) = match event {
            UserEvent::Api(req) => return self.api(req, el),
            UserEvent::BarText(n, s) => {
                if let Some(slot) = self.bar_texts.get_mut(n) {
                    *slot = s;
                    self.dirty = true;
                }
                return;
            }
            UserEvent::Term(id, ev) => (id, ev),
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
            TermEvent::Title(s) => {
                let lower = s.to_lowercase();
                if let Some(p) = self.panes.get_mut(&id) {
                    // shells often set their program path as the title; the folder name is more useful
                    p.title = if lower.ends_with(".exe") || lower.contains(r"\system32\") {
                        p.cwd.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(s)
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
                    p.write_str(f(tab::window_size(p.size, self.fonts.cell_w, self.fonts.cell_h)));
                }
            }
            TermEvent::ColorRequest(idx, f) => {
                let c = if idx < 16 { PALETTE[idx] } else if idx == 257 { self.theme.bg } else { self.theme.fg };
                if let Some(p) = self.panes.get(&id) {
                    p.write_str(f(Rgb { r: (c >> 16) as u8, g: (c >> 8) as u8, b: c as u8 }));
                }
            }
            TermEvent::ClipboardStore(_, s) => {
                if let Some(c) = self.clipboard.as_mut() {
                    let _ = c.set_text(s);
                }
            }
            TermEvent::ClipboardLoad(_, f) => {
                let text = self.clipboard.as_mut().and_then(|c| c.get_text().ok()).unwrap_or_default();
                if let Some(p) = self.panes.get(&id) {
                    p.write_str(f(&text));
                }
            }
            TermEvent::ChildExit(_) | TermEvent::Exit => self.close_pane(id, el),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        let mut soon = |t: Instant| next = Some(next.map_or(t, |n: Instant| n.min(t)));

        if let Some(rx) = &self.switching {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.switching = None;
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
        let busy = !self.waiters.is_empty() || self.panes.values().any(|p| p.det.recently_active() || p.det.state == State::Working);
        if busy {
            if now.duration_since(self.status_at) >= Duration::from_millis(500) {
                self.status_tick();
            }
            soon(self.status_at + Duration::from_millis(500));
            if let Some(d) = self.waiters.iter().map(|w| w.deadline).min() {
                soon(d);
            }
        }
        let before = self.toasts.len();
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(6));
        if self.toasts.len() != before {
            self.dirty = true;
        }
        if let Some(t) = self.toasts.iter().map(|t| t.at + Duration::from_secs(6)).min() {
            soon(t);
        }
        // settings: picked up when the file changes (checked every few seconds, only while the window is in front)
        if self.window_focused && !self.hidden && !self.screen_off {
            if self.cfg_checked.elapsed() >= Duration::from_secs(3) {
                self.reload_config_if_changed();
            }
            soon(self.cfg_checked + Duration::from_secs(3));
        }
        // keep the restore point fresh (only matters after activity)
        if busy && self.saved_at.elapsed() >= Duration::from_secs(60) {
            self.save_session();
        }
        if self.dirty && !self.screen_off && !self.hidden {
            let frame = self.last_draw + FRAME;
            if now >= frame {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            } else {
                soon(frame);
            }
        }
        el.set_control_flow(match next {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

// ---------------------------------------------------------------- command line

const HELP: &str = "\
Lights Out: the terminal for AI agents.

  lightsout                       open the app (or bring its window back)
  lightsout --session NAME        a separate, named set of tabs
  lightsout uninstall             remove Lights Out from this computer

Control the running app (also for agents; details: `lightsout skill`):
  lightsout list | new | send | read | wait | explain | focus | notify | screen | power | switch
";

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
    let after_dashes = flag("--").map(|i| rest[i + 1..].join(" "));
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
        "list" => ("list", json!({})),
        "new" => ("new", json!({ "cwd": value("--cwd"), "command": after_dashes.or_else(|| (!positional.is_empty()).then(|| positional.join(" "))), "split": split })),
        "send" => ("send", json!({ "pane": pane(0), "text": positional.iter().skip(1).cloned().collect::<Vec<_>>().join(" "), "enter": flag("--no-enter").is_none() })),
        "read" => ("read", json!({ "pane": pane(0), "lines": value("--lines").and_then(|v| v.parse::<u64>().ok()).unwrap_or(60) })),
        "wait" => ("wait", json!({ "pane": pane(0), "until": value("--until"), "timeout": value("--timeout").and_then(|v| v.parse::<u64>().ok()).unwrap_or(600) })),
        "explain" => ("explain", json!({ "pane": pane(0) })),
        "focus" => ("focus", json!({ "pane": pane(0) })),
        "notify" => ("notify", json!({ "text": positional.join(" ") })),
        "screen" => ("screen", json!({ "on": positional.first().map(|s| s != "off").unwrap_or(true) })),
        "power" => ("power", json!({})),
        "switch" | "windows" => ("switch", json!({})),
        "handback" => ("handback", json!({})),
        _ => {
            eprint!("{HELP}");
            return 2;
        }
    };
    match api::call(session, cmd, a) {
        Ok(v) => {
            match cmd {
                "list" => {
                    println!("{:<5} {:<12} {:<4} {:<10} {:<20} FOLDER", "PANE", "ROOM", "TAB", "STATE", "TITLE");
                    for p in v["panes"].as_array().cloned().unwrap_or_default() {
                        println!(
                            "{:<5} {:<12} {:<4} {:<10} {:<20} {}",
                            p["id"],
                            p["room"].as_str().unwrap_or("").chars().take(12).collect::<String>(),
                            p["tab"],
                            p["state"].as_str().unwrap_or(""),
                            p["title"].as_str().unwrap_or("").chars().take(20).collect::<String>(),
                            p["cwd"].as_str().unwrap_or("")
                        );
                    }
                }
                "new" => println!("{}", v["id"]),
                "read" => println!("{}", v["text"].as_str().unwrap_or("")),
                "wait" => println!("{}{}", v["state"].as_str().unwrap_or(""), if v["timeout"].as_bool() == Some(true) { " (timed out)" } else { "" }),
                "explain" => println!("{}: {}", v["state"].as_str().unwrap_or(""), v["reason"].as_str().unwrap_or("")),
                "power" => match v["percent"].as_u64() {
                    Some(p) => println!(
                        "battery {p}%{}{}",
                        v["watts"].as_f64().map(|w| format!(", drawing {w:.1} W")).unwrap_or_default(),
                        if v["charging"].as_bool() == Some(true) { ", charging" } else { "" }
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
    if rest.is_empty() {
        // open the app (or bring it back) without tying up this terminal
        let mut app = std::env::current_exe().unwrap_or_default();
        app.set_file_name(if cfg!(windows) { "lightsout-app.exe" } else { "lightsout-app" });
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
            _ => std::process::exit(client(session, rest)),
        }
    }
}

/// Removes Lights Out from this computer (asks for admin rights).
fn uninstall() -> i32 {
    let status = if cfg!(windows) {
        let script = platform::lightsout_dir().join("bin").join("uninstall.ps1");
        let cmd = format!(
            "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File \"{}\"'",
            script.display()
        );
        std::process::Command::new("powershell").args(["-NoProfile", "-Command", &cmd]).status()
    } else {
        std::process::Command::new("sudo").args(["sh", "/usr/local/lib/lightsout/uninstall.sh"]).status()
    };
    match status {
        Ok(s) if s.success() => 0,
        _ => {
            eprintln!("lightsout: uninstall did not finish");
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
    // Mac sign-in: open only when coming back from Lights Out
    if rest.first().map(String::as_str) == Some("--if-returning") && !platform::in_lightsout() {
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
    alacritty_terminal::tty::setup_env();
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let mut app = App::new(event_loop.create_proxy(), session);
    platform::lid_keep_running();
    let _ = event_loop.run_app(&mut app);
    platform::lid_restore();
}
