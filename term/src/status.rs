//! What an agent in a pane is doing, read from its screen. Works for any
//! program, no integration needed: working / needs you / done / idle.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Working,
    NeedsYou,
    Done,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::NeedsYou => "needs-you",
            State::Done => "done",
        }
    }
}

/// Lines that mean "the program is waiting for your answer". More can be added
/// in the config file ([status] needs_you = [...]).
const NEEDS_YOU: &[&str] = &[
    "do you want to",
    "(y/n)",
    "[y/n]",
    "(yes/no)",
    "yes/no?",
    "press enter to",
    "allow this",
    "approve",
    "❯ 1. yes",
    "waiting for your",
    "requires your approval",
    "permission to",
];
/// Signs that it is busy even when output pauses.
const BUSY: &[&str] = &["esc to interrupt", "ctrl+c to interrupt", "esc to cancel", "thinking", "working..."];
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '✻', '✽', '✶', '✳', '✢'];

pub struct Detector {
    pub state: State,
    pub reason: String,
    pub changed: Instant,
    last_output: Instant,
    working_since: Option<Instant>,
    bell: bool,
}

impl Default for Detector {
    fn default() -> Self {
        Detector {
            state: State::Idle,
            reason: "nothing has happened yet".into(),
            changed: Instant::now(),
            last_output: Instant::now() - Duration::from_secs(60),
            working_since: None,
            bell: false,
        }
    }
}

impl Detector {
    pub fn on_output(&mut self) {
        self.last_output = Instant::now();
    }

    pub fn on_bell(&mut self) {
        self.bell = true;
    }

    pub fn seen(&mut self) {
        self.bell = false;
        if self.state == State::Done {
            self.state = State::Idle;
            self.changed = Instant::now();
            self.reason = "you looked at it".into();
        }
    }

    pub fn recently_active(&self) -> bool {
        self.last_output.elapsed() < Duration::from_secs(3)
    }

    /// Re-reads the last lines of the screen. Returns the new state if it changed.
    pub fn update(&mut self, tail: &str, focused: bool, extra_needs: &[String]) -> Option<State> {
        let lower = tail.to_lowercase();
        let recent = self.last_output.elapsed() < Duration::from_millis(1500);
        let needs = NEEDS_YOU.iter().map(|s| s.to_string()).chain(extra_needs.iter().map(|s| s.to_lowercase())).find(|p| lower.contains(p.as_str()));
        let busy = BUSY.iter().find(|p| lower.contains(*p)).map(|s| s.to_string()).or_else(|| {
            tail.chars().find(|c| SPINNER.contains(c)).map(|c| format!("a spinner ({c}) is on screen"))
        });
        let (state, reason) = if let Some(p) = needs {
            (State::NeedsYou, format!("the screen asks: \"{p}\""))
        } else if self.bell && !focused {
            (State::NeedsYou, "the program rang the bell".into())
        } else if recent {
            (State::Working, "it is printing output".into())
        } else if let Some(b) = busy {
            (State::Working, format!("busy sign on screen: \"{b}\""))
        } else if self.state == State::Done && !focused {
            (State::Done, self.reason.clone())
        } else if self.working_since.is_some_and(|t| self.last_output.saturating_duration_since(t) >= Duration::from_secs(3)) && !focused {
            (State::Done, "it worked for a while and has gone quiet".into())
        } else {
            (State::Idle, "quiet, no question on screen".into())
        };
        match state {
            State::Working => {
                self.working_since.get_or_insert_with(Instant::now);
            }
            _ => self.working_since = None,
        }
        self.reason = reason;
        if state != self.state {
            self.state = state;
            self.changed = Instant::now();
            Some(state)
        } else {
            None
        }
    }
}
