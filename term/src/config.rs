//! One settings file, shared by Windows and Lights Out:
//! ~/.config/lightsout/config.toml. Changes apply without restarting.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

pub const TEMPLATE: &str = r##"# Lights Out settings. Saved changes apply right away.

[theme]
background = "#000000"   # full black: OLED pixels are switched off
foreground = "#d6d6d6"
accent = "#f5c542"
font_size = 0            # 0 = automatic

[notify]
popup = true             # a popup when an agent finishes or needs you
sound = false            # a short sound with it (Windows, Mac)
sound_done = ""          # optional .wav files
sound_needs_you = ""

[status]
# extra phrases that mean "this program is waiting for my answer"
needs_you = []

[history]
keep = true              # bring tabs, splits and screen history back after a restart

[bar]
# extra items on the right of the top bar, e.g.
# items = [ { text = "my laptop" }, { command = "git branch --show-current", every = 30 } ]
items = []

# Your own keys: a command in a popup, a new tab or a split
# [keys]
# "ctrl+alt+g" = { command = "lazygit", mode = "popup" }
# "ctrl+alt+t" = { command = "npm test", mode = "split" }
"##;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub notify: Notify,
    pub status: Status,
    pub history: History,
    pub bar: Bar,
    pub keys: HashMap<String, KeyCmd>,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct Theme {
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub font_size: f32,
}
impl Default for Theme {
    fn default() -> Self {
        Theme {
            background: "#000000".into(),
            foreground: "#d6d6d6".into(),
            accent: "#f5c542".into(),
            font_size: 0.0,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct Notify {
    pub popup: bool,
    pub sound: bool,
    pub sound_done: String,
    pub sound_needs_you: String,
}
impl Default for Notify {
    fn default() -> Self {
        Notify {
            popup: true,
            sound: false,
            sound_done: String::new(),
            sound_needs_you: String::new(),
        }
    }
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Status {
    pub needs_you: Vec<String>,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct History {
    pub keep: bool,
}
impl Default for History {
    fn default() -> Self {
        History { keep: true }
    }
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Bar {
    pub items: Vec<BarItem>,
}

#[derive(Deserialize, Default, Clone)]
pub struct BarItem {
    pub text: Option<String>,
    pub command: Option<String>,
    #[serde(default = "thirty")]
    pub every: u64,
}
fn thirty() -> u64 {
    30
}

#[derive(Deserialize, Clone)]
pub struct KeyCmd {
    pub command: String,
    #[serde(default = "popup")]
    pub mode: String,
}
fn popup() -> String {
    "popup".into()
}

pub fn path() -> PathBuf {
    crate::platform::home()
        .join(".config")
        .join("lightsout")
        .join("config.toml")
}

pub fn color(s: &str, fallback: u32) -> u32 {
    u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(fallback)
}

pub struct Loaded {
    pub config: Config,
    pub modified: Option<SystemTime>,
    pub error: Option<String>,
}

/// Reads the settings (creating the commented template the first time).
pub fn load() -> Loaded {
    let p = path();
    if !p.exists() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, TEMPLATE);
    }
    let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    match std::fs::read_to_string(&p).map(|s| toml::from_str::<Config>(&s)) {
        Ok(Ok(config)) => Loaded {
            config,
            modified,
            error: None,
        },
        Ok(Err(e)) => Loaded {
            config: Config::default(),
            modified,
            error: Some(format!("settings file: {}", e.message())),
        },
        Err(_) => Loaded {
            config: Config::default(),
            modified,
            error: None,
        },
    }
}

pub fn changed_since(t: Option<SystemTime>) -> bool {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok() != t
}
