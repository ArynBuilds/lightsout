//! Lights Out inside the terminal it was typed in (`lightsout` in PowerShell, cmd, bash, zsh...),
//! the way tmux runs: same rooms, tabs, panes, sidebar and actions as the window, drawn in cells.

use std::io::{self, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    MouseButton as TMouse, MouseEventKind,
};
use crossterm::style::{
    Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::{cursor, execute, queue, terminal};
use unicode_width::UnicodeWidthChar;
use winit::dpi::PhysicalPosition;
use winit::event::MouseButton;
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use super::*;

const LINE: u32 = 0x3a3a42; // lines between things: a bit brighter than the window's, terminals render thin
const HEAD: u32 = 0x6a6a74;

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    c: char, // '\0' = the right half of a wide character
    fg: u32,
    bg: u32,
    bold: bool,
    under: bool,
}

/// One frame of cells.
struct Grid {
    w: i32,
    h: i32,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(w: i32, h: i32, bg: u32) -> Self {
        let blank = Cell {
            c: ' ',
            fg: 0xffffff,
            bg,
            bold: false,
            under: false,
        };
        Grid {
            w,
            h,
            cells: vec![blank; (w.max(0) * h.max(0)) as usize],
        }
    }

    fn at(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        self.cells.get_mut((y * self.w + x) as usize)
    }

    fn fill(&mut self, r: Rect, bg: u32) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                if let Some(c) = self.at(x, y) {
                    *c = Cell {
                        c: ' ',
                        fg: c.fg,
                        bg,
                        bold: false,
                        under: false,
                    };
                }
            }
        }
    }

    /// One character; answers how many columns it took.
    fn put(&mut self, x: i32, y: i32, ch: char, fg: u32, bold: bool) -> i32 {
        let wide = ch.width().unwrap_or(1) > 1;
        if wide && x + 1 >= self.w {
            return 1;
        }
        let Some(c) = self.at(x, y) else {
            return if wide { 2 } else { 1 };
        };
        c.c = if ch.is_control() { ' ' } else { ch };
        c.fg = fg;
        c.bold = bold;
        c.under = false;
        if wide {
            let bg = c.bg;
            if let Some(n) = self.at(x + 1, y) {
                *n = Cell {
                    c: '\0',
                    fg,
                    bg,
                    bold,
                    under: false,
                };
            }
            return 2;
        }
        1
    }

    /// Text from x, cut at `max_x`; answers where it ended.
    fn text(&mut self, x: i32, y: i32, s: &str, fg: u32, bold: bool, max_x: i32) -> i32 {
        let mut x = x;
        for ch in s.chars() {
            let cw = ch.width().unwrap_or(0) as i32;
            if cw == 0 {
                continue;
            }
            if x + cw > max_x {
                break;
            }
            x += self.put(x, y, ch, fg, bold);
        }
        x
    }

    fn hline(&mut self, x: i32, y: i32, w: i32, fg: u32) {
        for i in 0..w {
            self.put(x + i, y, '─', fg, false);
        }
    }

    fn vline(&mut self, x: i32, y: i32, h: i32, fg: u32) {
        for i in 0..h {
            self.put(x, y + i, '│', fg, false);
        }
    }

    /// A frame around `r` (drawn just outside it), with `r` cleared.
    fn frame(&mut self, r: Rect, bg: u32) {
        self.fill(
            Rect {
                x: r.x - 1,
                y: r.y - 1,
                w: r.w + 2,
                h: r.h + 2,
            },
            bg,
        );
        self.hline(r.x, r.y - 1, r.w, LINE);
        self.hline(r.x, r.y + r.h, r.w, LINE);
        self.vline(r.x - 1, r.y, r.h, LINE);
        self.vline(r.x + r.w, r.y, r.h, LINE);
        self.put(r.x - 1, r.y - 1, '┌', LINE, false);
        self.put(r.x + r.w, r.y - 1, '┐', LINE, false);
        self.put(r.x - 1, r.y + r.h, '└', LINE, false);
        self.put(r.x + r.w, r.y + r.h, '┘', LINE, false);
    }
}

fn width(s: &str) -> i32 {
    s.chars().map(|c| c.width().unwrap_or(0) as i32).sum()
}

fn cut(s: &str, max: i32) -> String {
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0) as i32;
        if w + cw > max {
            break;
        }
        w += cw;
        out.push(c);
    }
    out
}

fn rgb(c: u32) -> Color {
    Color::Rgb {
        r: (c >> 16) as u8,
        g: (c >> 8) as u8,
        b: c as u8,
    }
}

/// The terminal we draw into: what it shows now, so only changed rows are sent.
struct Screen {
    /// Ctrl+B was pressed: the next key is a Lights Out shortcut (like tmux).
    prefix: bool,
    shown: Vec<Cell>,
    w: i32,
    cursor: Option<(u16, u16, CursorShape)>,
}

impl Screen {
    fn present(&mut self, g: &Grid, cursor: Option<(u16, u16, CursorShape)>) -> io::Result<()> {
        let mut out: Vec<u8> = Vec::with_capacity(32 * 1024);
        queue!(out, terminal::BeginSynchronizedUpdate, cursor::Hide)?;
        let full = self.w != g.w || self.shown.len() != g.cells.len();
        let (mut fg, mut bg, mut bold, mut under) = (None, None, false, false);
        queue!(out, SetAttribute(Attribute::Reset))?;
        for y in 0..g.h {
            let row = &g.cells[(y * g.w) as usize..((y + 1) * g.w) as usize];
            if !full && &self.shown[(y * g.w) as usize..((y + 1) * g.w) as usize] == row {
                continue;
            }
            queue!(out, cursor::MoveTo(0, y as u16))?;
            let mut covered = false;
            for c in row {
                if std::mem::take(&mut covered) {
                    continue; // the wide character before it covered this column
                }
                // a stray right half (its wide character was drawn over) shows as a space
                let ch = if c.c == '\0' { ' ' } else { c.c };
                covered = ch.width().unwrap_or(1) > 1;
                if bold != c.bold || under != c.under {
                    queue!(out, SetAttribute(Attribute::Reset))?;
                    if c.bold {
                        queue!(out, SetAttribute(Attribute::Bold))?;
                    }
                    if c.under {
                        queue!(out, SetAttribute(Attribute::Underlined))?;
                    }
                    (bold, under, fg, bg) = (c.bold, c.under, None, None);
                }
                if fg != Some(c.fg) {
                    queue!(out, SetForegroundColor(rgb(c.fg)))?;
                    fg = Some(c.fg);
                }
                if bg != Some(c.bg) {
                    queue!(out, SetBackgroundColor(rgb(c.bg)))?;
                    bg = Some(c.bg);
                }
                queue!(out, Print(ch))?;
            }
        }
        queue!(out, SetAttribute(Attribute::Reset))?;
        if let Some((x, y, shape)) = cursor {
            let style = match shape {
                CursorShape::Beam => cursor::SetCursorStyle::SteadyBar,
                CursorShape::Underline => cursor::SetCursorStyle::SteadyUnderScore,
                _ => cursor::SetCursorStyle::SteadyBlock,
            };
            if self.cursor.map(|c| c.2) != Some(shape) {
                queue!(out, style)?;
            }
            queue!(out, cursor::MoveTo(x, y), cursor::Show)?;
        }
        queue!(out, terminal::EndSynchronizedUpdate)?;
        let mut stdout = io::stdout().lock();
        stdout.write_all(&out)?;
        stdout.flush()?;
        self.shown.clone_from(&g.cells);
        self.w = g.w;
        self.cursor = cursor;
        Ok(())
    }
}

/// Puts the terminal back the way it was (also after a crash).
fn restore_terminal() {
    let mut out = io::stdout();
    let _ = execute!(out, event::PopKeyboardEnhancementFlags);
    let _ = execute!(
        out,
        event::DisableMouseCapture,
        event::DisableBracketedPaste,
        event::DisableFocusChange,
        SetAttribute(Attribute::Reset),
        cursor::SetCursorStyle::DefaultUserShape,
        cursor::Show,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

/// Runs Lights Out in this terminal until it quits. Answers the exit code.
pub fn run(session: &str) -> i32 {
    let (tx, rx) = mpsc::channel::<UserEvent>();
    let mut app = App::new(Waker::Channel(tx.clone()), session.to_string());
    app.tui = true;
    app.fonts.use_cells();
    let (c, r) = terminal::size().unwrap_or((100, 30));
    app.size = (c.max(20) as u32, r.max(8) as u32);

    if terminal::enable_raw_mode().is_err() {
        eprintln!("lightsout: this terminal can't be used; opening the window instead (lightsout --window)");
        return 1;
    }
    // NO_COLOR is about a program's printed output; a full-screen app (like tmux) still draws
    // its frame and the panes' own colors. The panes still see NO_COLOR and honour it themselves.
    crossterm::style::Colored::set_ansi_color_disabled(false);
    let mut out = io::stdout();
    let _ = execute!(
        out,
        terminal::EnterAlternateScreen,
        cursor::Hide,
        event::EnableMouseCapture,
        event::EnableFocusChange
    );
    let _ = execute!(out, event::EnableBracketedPaste);
    // terminals that can tell Ctrl+Shift+P from Ctrl+P (kitty, WezTerm, foot, Ghostty...)
    if terminal::supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(
            out,
            event::PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        );
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        hook(info);
    }));

    // keys, mouse and resizes come in on their own thread, next to the panes' events
    let input = tx;
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if input.send(UserEvent::Tty(ev)).is_err() {
                break;
            }
        }
    });

    if api::serve(&app.session, app.proxy.clone()).is_err() {
        app.toast(
            "Control commands are unavailable (could not open the local port)".into(),
            ALERT,
        );
    }
    app.start();
    app.dirty = true;

    let mut screen = Screen {
        prefix: false,
        shown: Vec::new(),
        w: 0,
        cursor: None,
    };
    loop {
        let next = app.tick();
        if app.quit {
            break;
        }
        if std::mem::take(&mut app.redraw) {
            app.refresh_battery();
            let (g, cursor) = draw(&mut app, screen.prefix);
            if screen.present(&g, cursor).is_err() {
                break; // the terminal went away
            }
            app.dirty = false;
            app.last_draw = Instant::now();
        }
        let wait = next
            .map(|t| t.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            Ok(ev) => handle(&mut app, &mut screen, ev),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // take everything that is already waiting before drawing again
        while !app.quit {
            match rx.try_recv() {
                Ok(ev) => handle(&mut app, &mut screen, ev),
                Err(_) => break,
            }
        }
        if app.quit {
            break;
        }
    }
    app.save_session();
    restore_terminal();
    0
}

fn handle(app: &mut App, screen: &mut Screen, ev: UserEvent) {
    let UserEvent::Tty(ev) = ev else {
        app.on_user_event(ev);
        return;
    };
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            if app.screen_off {
                platform::screen(true);
                app.screen_off = false;
                app.dirty = true;
                return;
            }
            // F1 opens (and closes) actions in any terminal, even where Ctrl+Shift+P can't get through
            if k.code == KeyCode::F(1)
                && k.modifiers.is_empty()
                && !matches!(app.lock, Lock::Locked { .. })
            {
                if matches!(app.overlay, Overlay::None) {
                    app.open_palette();
                } else if matches!(app.overlay, Overlay::Palette { .. }) {
                    app.overlay = Overlay::None;
                }
                app.dirty = true;
                return;
            }
            if !matches!(app.lock, Lock::Locked { .. }) {
                if std::mem::take(&mut screen.prefix) {
                    prefixed(app, &k);
                    app.dirty = true;
                    return;
                }
                if k.code == KeyCode::Char('b') && k.modifiers == KeyModifiers::CONTROL {
                    screen.prefix = true;
                    app.dirty = true;
                    return;
                }
            }
            if let Some((key, text, mods)) = convert(&k) {
                app.mods = mods;
                app.key_press(&key, text.as_deref());
                app.mods = ModifiersState::empty();
            }
        }
        Event::Key(_) => {}
        Event::Mouse(m) => {
            app.mouse = PhysicalPosition::new(m.column as f64, m.row as f64);
            app.mods = ModifiersState::empty();
            if m.modifiers.contains(KeyModifiers::SHIFT) {
                app.mods |= ModifiersState::SHIFT;
            }
            if m.modifiers.contains(KeyModifiers::CONTROL) {
                app.mods |= ModifiersState::CONTROL;
            }
            match m.kind {
                MouseEventKind::Down(_) if app.screen_off => {
                    platform::screen(true);
                    app.screen_off = false;
                    app.dirty = true;
                }
                MouseEventKind::Down(b) => {
                    app.mouse_down(match b {
                        TMouse::Left => MouseButton::Left,
                        TMouse::Right => MouseButton::Right,
                        TMouse::Middle => MouseButton::Middle,
                    });
                    keep_menu_inside(app);
                }
                MouseEventKind::Up(_) => app.mouse_up(),
                MouseEventKind::Drag(_) | MouseEventKind::Moved => app.mouse_move(),
                MouseEventKind::ScrollUp => app.on_wheel(3),
                MouseEventKind::ScrollDown => app.on_wheel(-3),
                _ => {}
            }
            app.mods = ModifiersState::empty();
        }
        Event::Paste(s) => app.paste_text(s),
        Event::Resize(c, r) => {
            app.size = (c.max(20) as u32, r.max(8) as u32);
            app.resize_all();
            screen.shown.clear(); // everything again
            app.dirty = true;
        }
        Event::FocusGained => {
            app.window_focused = true;
            if let Some(id) = app.focused() {
                if let Some(p) = app.panes.get_mut(&id) {
                    p.det.seen();
                }
            }
            app.reload_config_if_changed();
        }
        Event::FocusLost => app.window_focused = false,
    }
}

/// A right-click menu opens where the mouse is; keep it on screen.
fn keep_menu_inside(app: &mut App) {
    let (w, h) = (app.size.0 as i32, app.size.1 as i32);
    if let Overlay::Menu { x, y, items, .. } = &mut app.overlay {
        let mw = items
            .iter()
            .map(|i| i.label.chars().count() + i.hint.chars().count() + 4)
            .max()
            .unwrap_or(10) as i32;
        let mh = items.len() as i32 + 2;
        *x = (*x).min(w - mw - 1).max(1);
        *y = (*y).min(h - mh).max(0);
    }
}

/// A key from the terminal, as the window would have seen it: what was pressed, what it types, modifiers.
fn convert(k: &KeyEvent) -> Option<(Key, Option<String>, ModifiersState)> {
    let mut m = ModifiersState::empty();
    let km = k.modifiers;
    if km.contains(KeyModifiers::SHIFT) {
        m |= ModifiersState::SHIFT;
    }
    if km.contains(KeyModifiers::CONTROL) {
        m |= ModifiersState::CONTROL;
    }
    if km.contains(KeyModifiers::ALT) {
        m |= ModifiersState::ALT;
    }
    if km.contains(KeyModifiers::SUPER) {
        m |= ModifiersState::SUPER;
    }
    let named = |n: NamedKey| Some((Key::Named(n), None, m));
    match k.code {
        KeyCode::Char(' ') => Some((Key::Named(NamedKey::Space), Some(" ".into()), m)),
        KeyCode::Char(c) => {
            // AltGr comes as Ctrl+Alt on Windows: it types a character (@, €, {...), not a shortcut
            if m.control_key() && m.alt_key() && !c.is_ascii_alphanumeric() {
                m = ModifiersState::empty();
            }
            let s = c.to_string();
            let text = if m.control_key() {
                None
            } else {
                Some(s.clone())
            };
            Some((Key::Character(SmolStr::new(&s)), text, m))
        }
        KeyCode::Enter => named(NamedKey::Enter),
        KeyCode::Backspace => named(NamedKey::Backspace),
        KeyCode::Tab => named(NamedKey::Tab),
        KeyCode::BackTab => {
            m |= ModifiersState::SHIFT;
            Some((Key::Named(NamedKey::Tab), None, m))
        }
        KeyCode::Esc => named(NamedKey::Escape),
        KeyCode::Up => named(NamedKey::ArrowUp),
        KeyCode::Down => named(NamedKey::ArrowDown),
        KeyCode::Left => named(NamedKey::ArrowLeft),
        KeyCode::Right => named(NamedKey::ArrowRight),
        KeyCode::Home => named(NamedKey::Home),
        KeyCode::End => named(NamedKey::End),
        KeyCode::PageUp => named(NamedKey::PageUp),
        KeyCode::PageDown => named(NamedKey::PageDown),
        KeyCode::Insert => named(NamedKey::Insert),
        KeyCode::Delete => named(NamedKey::Delete),
        KeyCode::F(n) => named(match n {
            1 => NamedKey::F1,
            2 => NamedKey::F2,
            3 => NamedKey::F3,
            4 => NamedKey::F4,
            5 => NamedKey::F5,
            6 => NamedKey::F6,
            7 => NamedKey::F7,
            8 => NamedKey::F8,
            9 => NamedKey::F9,
            10 => NamedKey::F10,
            11 => NamedKey::F11,
            12 => NamedKey::F12,
            _ => return None,
        }),
        _ => None,
    }
}

/// One frame: the same layout as the window, in cells. Also records what the mouse can hit.
fn draw(app: &mut App, prefix: bool) -> (Grid, Option<(u16, u16, CursorShape)>) {
    let t = app.theme;
    let (w, h) = (app.size.0 as i32, app.size.1 as i32);
    let mut g = Grid::new(w, h, t.bg);
    app.hits.clear();
    app.scroll_areas.clear();

    if let Lock::Locked { wrong, .. } = &app.lock {
        let center = |s: &str| (w - width(s)) / 2;
        let cy = h / 2 - 2;
        let s = "L I G H T S   O U T";
        g.text(center(s), cy, s, t.fg, true, w);
        let s = if *wrong > 0 {
            "Wrong PIN. Try again."
        } else {
            "Locked. Type your PIN and press Enter."
        };
        g.text(
            center(s),
            cy + 2,
            s,
            if *wrong > 0 { t.fg } else { DIMTEXT },
            false,
            w,
        );
        let dots = "●".repeat(app.pin.chars().count());
        g.text(center(&dots), cy + 4, &dots, t.fg, false, w);
        return (g, None);
    }

    let (tab_states, rooms) = app.summary();
    let sw = app.sidebar_w();
    let content = app.content();
    let lo = platform::in_lightsout();

    // ---- top bar: tabs left (they scroll), your items right
    let mut right: Vec<(String, Option<Hit>)> = app
        .bar_texts
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| (s.clone(), None))
        .collect();
    if sw == 0 {
        right.push((app.battery_text.clone(), None));
        right.push((
            if lo { "Lights On" } else { "Lights Out" }.into(),
            Some(Hit::Switch),
        ));
        right.push(("actions".into(), Some(Hit::Palette)));
    }
    if prefix {
        right.push((
            "Ctrl+B › press a key (P = actions)".into(),
            Some(Hit::Palette),
        ));
    }
    right.retain(|(s, _)| !s.is_empty());
    let right_w: i32 = right.iter().map(|(s, _)| width(s) + 2).sum();
    let labels: Vec<(String, i32)> = tab_states
        .iter()
        .enumerate()
        .map(|(i, (title, _))| {
            let label = format!(" {} {} ", i + 1, cut(title, 24));
            let lw = width(&label) + 2;
            (label, lw)
        })
        .collect();
    let x0 = sw + 1;
    let limit = w - right_w - 5;
    if app.active != app.seen_tab {
        app.seen_tab = app.active;
        if app.active < app.tabs_first {
            app.tabs_first = app.active;
        }
        while app.tabs_first < app.active {
            let used: i32 = labels[app.tabs_first..=app.active]
                .iter()
                .map(|l| l.1)
                .sum::<i32>()
                + 2;
            if x0 + used <= limit {
                break;
            }
            app.tabs_first += 1;
        }
    }
    app.tabs_first = app.tabs_first.min(labels.len().saturating_sub(1));
    let mut x = x0;
    if app.tabs_first > 0 {
        g.put(x, 0, '‹', t.fg, false);
        app.hits.push((
            Rect {
                x,
                y: 0,
                w: 2,
                h: 1,
            },
            Hit::TabsLeft,
        ));
        x += 2;
    }
    let mut more = false;
    for (i, (label, lw)) in labels.iter().enumerate().skip(app.tabs_first) {
        if x + lw > limit {
            more = true;
            break;
        }
        let (mark, mark_color) = state_mark(tab_states[i].1);
        g.put(x, 0, mark, mark_color, false);
        let on = i == app.active;
        g.text(x + 1, 0, label, if on { t.fg } else { DIMTEXT }, on, w);
        app.hits.push((
            Rect {
                x,
                y: 0,
                w: *lw,
                h: 1,
            },
            Hit::Tab(i),
        ));
        x += lw;
    }
    if more {
        g.put(x, 0, '›', t.fg, false);
        app.hits.push((
            Rect {
                x,
                y: 0,
                w: 2,
                h: 1,
            },
            Hit::TabsRight,
        ));
        x += 2;
    }
    g.text(x, 0, " + ", DIMTEXT, false, w);
    app.hits.push((
        Rect {
            x,
            y: 0,
            w: 3,
            h: 1,
        },
        Hit::NewTab,
    ));
    app.scroll_areas.push((
        Rect {
            x: sw,
            y: 0,
            w: w - sw,
            h: 1,
        },
        Scroll::Tabs,
    ));
    let mut rx = w - 1;
    for (s, hit) in right.into_iter().rev() {
        let sw_ = width(&s);
        rx -= sw_;
        if rx < x + 4 {
            break;
        }
        g.text(
            rx,
            0,
            &s,
            if hit.is_some() { t.fg } else { DIMTEXT },
            false,
            w,
        );
        if let Some(hh) = hit {
            app.hits.push((
                Rect {
                    x: rx,
                    y: 0,
                    w: sw_,
                    h: 1,
                },
                hh,
            ));
        }
        rx -= 2;
    }

    // ---- sidebar: rooms on top, what runs in this room below, switch and actions at the bottom
    if sw > 0 {
        g.vline(sw - 1, 0, h, LINE);
        let inner = sw - 2; // text from column 1 to sw-2
        let bottom = h - 3; // battery and buttons take the last two rows
        let mid = bottom / 2;
        // rooms
        g.text(1, 0, "rooms", HEAD, false, sw - 1);
        let y0 = 2;
        let fit = (mid - y0).max(1) as usize;
        let total = rooms.len() + 1;
        if app.room != app.seen_room {
            app.seen_room = app.room;
            if app.room < app.rooms_top {
                app.rooms_top = app.room;
            } else if app.room >= app.rooms_top + fit {
                app.rooms_top = app.room + 1 - fit;
            }
        }
        app.rooms_top = app.rooms_top.min(total.saturating_sub(fit));
        let mut y = y0;
        for i in app.rooms_top..total.min(app.rooms_top + fit) {
            if let Some((name, _)) = rooms.get(i) {
                let on = i == app.room;
                g.text(
                    1,
                    y,
                    &cut(name, inner),
                    if on { t.fg } else { DIMTEXT },
                    on,
                    sw - 1,
                );
                app.hits.push((
                    Rect {
                        x: 0,
                        y,
                        w: sw - 1,
                        h: 1,
                    },
                    Hit::Room(i),
                ));
            } else {
                g.text(1, y, "+ room", HEAD, false, sw - 1);
                app.hits.push((
                    Rect {
                        x: 0,
                        y,
                        w: sw - 1,
                        h: 1,
                    },
                    Hit::NewRoom,
                ));
            }
            y += 1;
        }
        marks(
            &mut g,
            sw - 3,
            0,
            app.rooms_top > 0,
            app.rooms_top + fit < total,
        );
        app.scroll_areas.push((
            Rect {
                x: 0,
                y: 0,
                w: sw,
                h: mid,
            },
            Scroll::Rooms,
        ));
        // what runs here
        g.hline(1, mid, inner, LINE);
        g.text(1, mid + 1, "running", HEAD, false, sw - 1);
        let y0 = mid + 3;
        if let Some((_, agents)) = rooms.get(app.room) {
            let fit = (bottom - y0).max(1) as usize;
            app.running_top = app.running_top.min(agents.len().saturating_sub(fit));
            marks(
                &mut g,
                sw - 3,
                mid + 1,
                app.running_top > 0,
                app.running_top + fit < agents.len(),
            );
            app.scroll_areas.push((
                Rect {
                    x: 0,
                    y: mid,
                    w: sw,
                    h: bottom - mid,
                },
                Scroll::Running,
            ));
            let mut y = y0;
            for (id, title, state, folder) in agents.iter().skip(app.running_top).take(fit) {
                let (mark, mark_color) = state_mark(*state);
                g.put(1, y, mark, mark_color, false);
                let end = g.text(3, y, &cut(title, inner - 2), 0xc8c8d0, false, sw - 1);
                if !folder.is_empty() && folder != title && end + 3 < sw - 1 {
                    g.text(
                        end + 2,
                        y,
                        &cut(folder, sw - 1 - end - 2),
                        DIMTEXT,
                        false,
                        sw - 1,
                    );
                }
                app.hits.push((
                    Rect {
                        x: 0,
                        y,
                        w: sw - 1,
                        h: 1,
                    },
                    Hit::Pane(*id),
                ));
                y += 1;
            }
        }
        // bottom left
        g.text(1, h - 2, &app.battery_text, DIMTEXT, false, sw - 1);
        let label = if lo { "Lights On" } else { "Lights Out" };
        let end = g.text(1, h - 1, label, t.fg, false, sw - 1);
        app.hits.push((
            Rect {
                x: 1,
                y: h - 1,
                w: end - 1,
                h: 1,
            },
            Hit::Switch,
        ));
        let ax = end + 3;
        g.text(ax, h - 1, "actions", t.fg, false, sw - 1);
        app.hits.push((
            Rect {
                x: ax,
                y: h - 1,
                w: 7,
                h: 1,
            },
            Hit::Palette,
        ));
    }

    // ---- panes
    let focused = app.focused();
    let rects = app.active_rects();
    let dividers = app
        .tabs
        .get(app.active)
        .map(|tv| tv.layout.layout(content).1)
        .unwrap_or_default();
    let mut cursor = None;
    let multi = rects.len() > 1;
    for (id, r) in &rects {
        if let Some(p) = app.panes.get(id) {
            if let Some(c) = draw_pane(&mut g, p, *r, t, focused == Some(*id)) {
                cursor = Some(c);
            }
            if multi && focused != Some(*id) {
                let (mark, mark_color) = state_mark(p.det.state);
                g.put(r.x + r.w - 2, r.y, mark, mark_color, false);
            }
        }
    }
    for d in &dividers {
        if d.side_by_side {
            g.vline(d.at, d.area.y, d.area.h, LINE);
        } else {
            g.hline(d.area.x, d.at, d.area.w, LINE);
        }
    }
    if let Some(pid) = app.popup {
        let r = app.popup_rect();
        g.frame(r, t.bg);
        if let Some(p) = app.panes.get(&pid) {
            cursor = draw_pane(&mut g, p, r, t, focused == Some(pid)).or(cursor);
        }
    }

    // ---- overlays (the cursor hides while one is open)
    let sel = app.sel();
    match &app.overlay {
        Overlay::Palette {
            title,
            items,
            filter,
            picked,
            top,
            ..
        } => {
            cursor = None;
            let f_l = filter.to_lowercase();
            let shown: Vec<&Item> = items
                .iter()
                .filter(|i| f_l.is_empty() || i.label.to_lowercase().contains(&f_l))
                .collect();
            let pw = (w * 6 / 10).max(44).min(w - 4);
            let rows = (shown.len() as i32).min(h - 8).max(1);
            let r = Rect {
                x: (w - pw) / 2,
                y: ((h - rows - 3) / 2).max(2),
                w: pw,
                h: rows + 3,
            };
            g.frame(r, 0x000000);
            g.text(r.x + 1, r.y, title, t.fg, true, r.x + r.w - 1);
            g.text(
                r.x + 1,
                r.y + 1,
                &format!("› {filter}"),
                t.fg,
                false,
                r.x + r.w - 1,
            );
            app.palette_rows = rows as usize;
            let start = (*top).min(shown.len().saturating_sub(rows as usize));
            for (n, it) in shown.iter().enumerate().skip(start).take(rows as usize) {
                let y = r.y + 3 + (n - start) as i32;
                let on = *picked && n == sel;
                app.hits.push((
                    Rect {
                        x: r.x,
                        y,
                        w: r.w,
                        h: 1,
                    },
                    Hit::PaletteItem(n),
                ));
                let hint = terminal_hint(&it.hint);
                let hx = r.x + r.w - 1 - width(&hint);
                g.text(r.x + 1, y, if on { "›" } else { " " }, t.fg, true, r.x + 2);
                g.text(
                    r.x + 3,
                    y,
                    &it.label,
                    if on { t.fg } else { DIMTEXT },
                    on,
                    hx - 1,
                );
                g.text(hx, y, &hint, DIMTEXT, false, r.x + r.w);
            }
            // more above or below
            marks(
                &mut g,
                r.x + r.w - 3,
                r.y,
                start > 0,
                start + (rows as usize) < shown.len(),
            );
            // a click inside the box (not on a row) keeps it open
            app.hits.push((
                Rect {
                    x: r.x - 1,
                    y: r.y - 1,
                    w: r.w + 2,
                    h: r.h + 2,
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
            cursor = None;
            let mw = items
                .iter()
                .map(|i| i.label.chars().count() + i.hint.chars().count() + 4)
                .max()
                .unwrap_or(10) as i32;
            let r = Rect {
                x: *mx,
                y: my + 1,
                w: mw,
                h: items.len() as i32,
            };
            g.frame(r, 0x000000);
            for (n, it) in items.iter().enumerate() {
                let y = r.y + n as i32;
                let on = n == *sel;
                g.text(
                    r.x + 1,
                    y,
                    &it.label,
                    if on { t.fg } else { 0xa8a8b0 },
                    on,
                    r.x + r.w,
                );
                let hint = terminal_hint(&it.hint);
                g.text(
                    r.x + r.w - 1 - width(&hint),
                    y,
                    &hint,
                    DIMTEXT,
                    false,
                    r.x + r.w,
                );
            }
        }
        Overlay::Confirm(c) => {
            cursor = None;
            let s = match c {
                Confirm::BackToWindows if platform::image() => "Lights On? Windows starts again; your tabs come along. Enter = yes, Esc = no",
                Confirm::BackToWindows => "Lights On? The desktop comes back; your tabs come along. Enter = yes, Esc = no",
                Confirm::EnterLightsOut if cfg!(windows) => "Lights Out? Windows shuts down, your tabs reopen there. Enter = yes, Esc = no",
                Confirm::EnterLightsOut if cfg!(target_os = "macos") => "Lights Out? Other apps quit (unsaved ones ask first), the Mac goes quiet. Enter = yes, Esc = no",
                Confirm::EnterLightsOut => "Lights Out? The desktop and its apps close (save your work first). Enter = yes, Esc = no",
                Confirm::Quit => "Quit Lights Out? All agents stop (tabs come back next time). Enter = yes, Esc = no",
            };
            g.fill(
                Rect {
                    x: 0,
                    y: h - 1,
                    w,
                    h: 1,
                },
                0x000000,
            );
            g.hline(0, h - 2, w, LINE);
            g.text(1, h - 1, s, t.fg, true, w);
        }
        Overlay::Ask {
            title,
            text,
            purpose,
        } => {
            let text = if matches!(purpose, Purpose::WifiPassword(_)) {
                "•".repeat(text.chars().count())
            } else {
                text.clone()
            };
            g.fill(
                Rect {
                    x: 0,
                    y: h - 2,
                    w,
                    h: 2,
                },
                0x000000,
            );
            g.hline(0, h - 3, w, LINE);
            g.text(1, h - 2, title, t.fg, true, w);
            let end = g.text(1, h - 1, &format!("› {text}"), t.fg, false, w);
            cursor = Some((end.min(w - 1) as u16, (h - 1) as u16, CursorShape::Beam));
        }
        Overlay::None => {}
    }

    // ---- toasts, bottom right
    let mut ty = h - 1;
    for toast in app.toasts.iter().rev() {
        let tw = (width(&toast.text) + 2).min(w - 4);
        ty -= 3;
        if ty < 1 {
            break;
        }
        let r = Rect {
            x: w - tw - 2,
            y: ty + 1,
            w: tw,
            h: 1,
        };
        g.frame(r, 0x000000);
        g.text(r.x + 1, r.y, &toast.text, t.fg, false, r.x + r.w);
    }
    (g, cursor)
}

/// Terminals keep many Ctrl+Shift keys (and Ctrl+Tab, Ctrl+,) for themselves, so here
/// those shortcuts are Ctrl+B and then the key.
fn terminal_hint(h: &str) -> String {
    if let Some(k) = h.strip_prefix("Ctrl+Shift+") {
        return match k {
            "Tab" => "Ctrl+B Shift+Tab".into(),
            "Right" => "Ctrl+B →".into(),
            "Down" => "Ctrl+B ↓".into(),
            _ => format!("Ctrl+B {k}"),
        };
    }
    match h {
        "Ctrl+Tab" => "Ctrl+B Tab".into(),
        "Ctrl+," => "Ctrl+B ,".into(),
        _ => h.into(),
    }
}

/// The key after Ctrl+B: run it as the shortcut it stands for.
fn prefixed(app: &mut App, k: &KeyEvent) {
    let Some((key, _, typed)) = convert(k) else {
        return;
    };
    let m = match &key {
        // Ctrl+B twice: the program in the pane gets a real Ctrl+B
        Key::Character(c) if c.as_str().eq_ignore_ascii_case("b") => {
            app.mods = ModifiersState::CONTROL;
            app.key_press(&key, None);
            app.mods = ModifiersState::empty();
            return;
        }
        Key::Named(NamedKey::Escape) => return,
        Key::Named(NamedKey::Tab) => ModifiersState::CONTROL | (typed & ModifiersState::SHIFT),
        Key::Character(c) if c.as_str() == "," => ModifiersState::CONTROL,
        _ => ModifiersState::CONTROL | ModifiersState::SHIFT,
    };
    app.mods = m;
    let known = app.shortcut(&key);
    app.mods = ModifiersState::empty();
    if !known {
        app.toast(
            "Ctrl+B then: P actions, T new tab, W close, N new room, A agent".into(),
            DIMTEXT,
        );
    }
}

/// ↑ ↓ next to a heading: more above, more below.
fn marks(g: &mut Grid, x: i32, y: i32, up: bool, down: bool) {
    if up {
        g.put(x, y, '↑', 0x8a8a93, false);
    }
    if down {
        g.put(x + 1, y, '↓', 0x8a8a93, false);
    }
}

/// A pane's cells into `r`; answers where its cursor is, when it shows one.
fn draw_pane(
    g: &mut Grid,
    p: &Pane,
    r: Rect,
    t: Theme,
    focused: bool,
) -> Option<(u16, u16, CursorShape)> {
    let term = p.term.lock();
    let content = term.renderable_content();
    let colors = content.colors;
    let offset = content.display_offset as i32;
    let selection = content.selection;
    let rows = (p.size.rows as i32).min(r.h);
    let cols = (p.size.cols as i32).min(r.w);
    g.fill(
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        },
        t.bg,
    );
    for cell in content.display_iter {
        let row = cell.point.line.0 + offset;
        let col = cell.point.column.0 as i32;
        if row < 0 || row >= rows || col >= cols {
            continue;
        }
        let flags = cell.flags;
        if flags.contains(Flags::WIDE_CHAR_SPACER)
            || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let mut fg = resolve(cell.fg, colors, t);
        let mut bg = resolve(cell.bg, colors, t);
        if flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if selection.is_some_and(|s| s.contains(cell.point)) {
            bg = 0x3a4a6a;
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        let (x, y) = (r.x + col, r.y + row);
        let wide = flags.contains(Flags::WIDE_CHAR);
        if let Some(c) = g.at(x, y) {
            c.bg = bg;
        }
        if wide {
            if let Some(c) = g.at(x + 1, y) {
                c.bg = bg;
            }
        }
        // a wide character that would not fit stays out (the grid is exact; the terminal must agree)
        let ch = if wide && col + 1 >= cols { ' ' } else { cell.c };
        if wide && ch != ' ' {
            if let Some(c) = g.at(x, y) {
                c.c = ch;
                c.fg = fg;
                c.bold = flags.contains(Flags::BOLD);
                c.under = flags.intersects(Flags::ALL_UNDERLINES);
            }
            if let Some(c) = g.at(x + 1, y) {
                *c = Cell {
                    c: '\0',
                    fg,
                    bg,
                    bold: false,
                    under: false,
                };
            }
        } else {
            // a character the grid thinks is narrow but the terminal would draw wide would shift the row
            let ch = if ch.width().unwrap_or(1) > 1 { '?' } else { ch };
            if let Some(c) = g.at(x, y) {
                c.c = if ch.is_control() { ' ' } else { ch };
                c.fg = fg;
                c.bold = flags.contains(Flags::BOLD);
                c.under = flags.intersects(Flags::ALL_UNDERLINES);
            }
        }
    }
    let cur = content.cursor;
    if focused
        && content.mode.contains(TermMode::SHOW_CURSOR)
        && cur.shape != CursorShape::Hidden
        && offset == 0
    {
        let (x, y) = (r.x + cur.point.column.0 as i32, r.y + cur.point.line.0);
        if x < r.x + r.w && y < r.y + r.h && y >= r.y {
            return Some((x as u16, y as u16, cur.shape));
        }
    }
    None
}
