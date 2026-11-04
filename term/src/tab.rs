//! A pane: one terminal (shell or agent) with its own PTY. Tabs hold one or
//! more panes (see layout.rs).

use crate::status::Detector;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{self, Term};
use alacritty_terminal::tty;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

pub enum UserEvent {
    /// Keys, mouse and resizes of the terminal Lights Out runs in (`lightsout` in a shell).
    Tty(crossterm::event::Event),
    Term(u64, Event),
    Api(crate::api::Request),
    BarText(usize, String),
}

/// Where terminal events go: the window's event loop, or a channel (headless).
#[derive(Clone)]
pub enum Waker {
    Window(EventLoopProxy<UserEvent>),
    Channel(std::sync::mpsc::Sender<UserEvent>),
}

impl Waker {
    /// False once nobody listens any more (the app ended).
    pub fn send(&self, e: UserEvent) -> bool {
        match self {
            Waker::Window(p) => p.send_event(e).is_ok(),
            Waker::Channel(t) => t.send(e).is_ok(),
        }
    }
}

#[derive(Clone)]
pub struct Proxy {
    id: u64,
    waker: Waker,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        self.waker.send(UserEvent::Term(self.id, event));
    }
}

/// Rows at the bottom of a pane that don't count as work when they change.
const BOTTOM_ROWS: i32 = 6;

#[derive(Clone, Copy, PartialEq)]
pub struct Size {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

pub struct Pane {
    pub id: u64,
    pub title: String,
    pub term: Arc<FairMutex<Term<Proxy>>>,
    tx: EventLoopSender,
    pub cwd: PathBuf,
    /// What the pane was started with (None = plain shell).
    pub command: Option<String>,
    pub shell_pid: Option<u32>,
    pub det: Detector,
    pub size: Size,
    /// The program running inside the shell, e.g. "claude" (None = just the shell).
    pub running: Option<String>,
    /// Fingerprint of the screen without its bottom rows (see `body_changed`).
    body: u64,
}

pub fn window_size(size: Size, cell_w: u32, cell_h: u32) -> WindowSize {
    WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width: cell_w as u16,
        cell_height: cell_h as u16,
    }
}

impl Pane {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        id: u64,
        waker: &Waker,
        session: &str,
        cwd: PathBuf,
        program: String,
        args: Vec<String>,
        command: Option<String>,
        size: Size,
        cell: (u32, u32),
        extra_env: Vec<(String, String)>,
    ) -> std::io::Result<Pane> {
        let p = Proxy {
            id,
            waker: waker.clone(),
        };
        let term = Term::new(term::Config::default(), &size, p.clone());
        let term = Arc::new(FairMutex::new(term));
        let mut env = HashMap::new();
        env.insert("LIGHTSOUT".into(), "1".into());
        env.insert("LIGHTSOUT_PANE".into(), id.to_string());
        env.insert("LIGHTSOUT_SESSION".into(), session.to_string());
        env.extend(extra_env);
        #[allow(clippy::needless_update)] // Windows has one more field
        let options = tty::Options {
            shell: Some(tty::Shell::new(program, args)),
            working_directory: Some(cwd.clone()),
            drain_on_exit: true,
            env,
            ..Default::default()
        };
        let pty = tty::new(&options, window_size(size, cell.0, cell.1), id)?;
        #[cfg(windows)]
        let shell_pid = pty.child_watcher().pid().map(|p| p.get());
        #[cfg(unix)]
        let shell_pid = Some(pty.child().id());
        let event_loop = EventLoop::new(term.clone(), p, pty, true, false)?;
        let tx = event_loop.channel();
        event_loop.spawn();
        let folder = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "shell".into());
        let title = command
            .as_deref()
            .and_then(|c| c.split_whitespace().next())
            .map(str::to_string)
            .unwrap_or(folder);
        Ok(Pane {
            id,
            title,
            term,
            tx,
            cwd,
            command,
            shell_pid,
            det: Detector::default(),
            size,
            running: None,
            body: 0,
        })
    }

    pub fn input(&self, bytes: Vec<u8>) {
        self.term.lock().scroll_display(Scroll::Bottom);
        let _ = self.tx.send(Msg::Input(bytes.into()));
    }

    pub fn write_str(&self, s: String) {
        let _ = self.tx.send(Msg::Input(s.into_bytes().into()));
    }

    pub fn resize(&mut self, size: Size, cell: (u32, u32)) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        let _ = self.tx.send(Msg::Resize(window_size(size, cell.0, cell.1)));
    }

    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }

    fn lines(&self, from: i32, to: i32) -> Vec<String> {
        let term = self.term.lock();
        let grid = term.grid();
        let cols = grid.columns();
        (from..to)
            .map(|l| {
                let row = &grid[Line(l)];
                (0..cols)
                    .filter(|&c| !row[Column(c)].flags.contains(Flags::WIDE_CHAR_SPACER))
                    .map(|c| row[Column(c)].c)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn trim(mut lines: Vec<String>) -> String {
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        while lines.first().is_some_and(|l| l.is_empty()) {
            lines.remove(0);
        }
        lines.join("\n")
    }

    /// The last `n` lines of history + screen (for `lightsout read`, saving, the editor).
    pub fn text(&self, n: usize) -> String {
        let (history, rows) = {
            let t = self.term.lock();
            (
                t.grid().history_size() as i32,
                t.grid().screen_lines() as i32,
            )
        };
        // count from the last line with text, not from the empty rows below the prompt
        let mut lines = self.lines((-(n as i32)).max(-history), rows);
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let start = lines.len().saturating_sub(n);
        Self::trim(lines.split_off(start))
    }

    /// The last few non-empty lines, for reading the agent's state.
    /// Whether the main part of the screen changed since the last look. The bottom
    /// rows are left out: agents redraw their input box, status line and spinners
    /// there all the time (also when another session works), which is not work.
    pub fn body_changed(&mut self) -> bool {
        use std::hash::{Hash, Hasher};
        let rows = self.term.lock().grid().screen_lines() as i32;
        let lines = self.lines(0, (rows - BOTTOM_ROWS).max(1));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        lines.hash(&mut h);
        let now = h.finish();
        let changed = now != self.body;
        self.body = now;
        changed
    }

    pub fn tail(&self) -> String {
        let rows = self.term.lock().grid().screen_lines() as i32;
        let lines: Vec<String> = self
            .lines(0, rows)
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .collect();
        lines[lines.len().saturating_sub(8)..].join("\n")
    }
}
