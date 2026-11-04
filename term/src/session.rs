//! Saving and restoring tabs, splits, commands and screens. The same format is
//! used for app restarts and for switching between Windows and Lights Out.

use crate::layout::Node;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct Saved {
    /// Older single-room format (still read).
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub rooms: Vec<SavedRoom>,
    #[serde(default)]
    pub room: usize,
}

/// A room: its own set of tabs.
#[derive(Serialize, Deserialize, Default)]
pub struct SavedRoom {
    pub name: String,
    pub tabs: Vec<SavedTab>,
    #[serde(default)]
    pub active: usize,
    /// A room that lives on another machine (None = this one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteRoom>,
}

/// A room on another machine: shown here, its agents run there.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct RemoteRoom {
    /// The machine's name from the settings ([[machine]] name).
    pub machine: String,
    /// How to reach it (`ssh` target).
    pub ssh: String,
    /// The room's name over there.
    pub room: String,
}

#[derive(Serialize, Deserialize)]
pub struct SavedTab {
    /// Leaves are indexes into `panes`.
    pub layout: Node,
    pub panes: Vec<SavedPane>,
    #[serde(default)]
    pub focus: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedPane {
    pub cwd: String,
    /// What was running in the pane, exactly as started (any program).
    #[serde(default)]
    pub command: Option<String>,
    /// Text the pane showed, shown dimmed at the top when it comes back.
    #[serde(default)]
    pub screen: Option<String>,
}

pub fn local_file(session: &str) -> PathBuf {
    crate::platform::data_dir().join(format!("session-{session}.json"))
}

pub fn save(path: &PathBuf, s: &Saved) {
    if let Ok(text) = serde_json::to_string(s) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

pub fn load(path: &PathBuf) -> Option<Saved> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}
