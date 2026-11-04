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

/// A room that lives on a server (`lightsout room add me@server`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ServerRoom {
    pub name: String,
    /// How to reach it (`ssh` target: user@host or a name from ~/.ssh/config).
    pub ssh: String,
}

fn server_rooms_file() -> std::path::PathBuf {
    crate::platform::data_dir().join("server-rooms.json")
}

pub fn server_rooms() -> Vec<ServerRoom> {
    std::fs::read_to_string(server_rooms_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_server_rooms(m: &[ServerRoom]) -> std::io::Result<()> {
    let f = server_rooms_file();
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(f, serde_json::to_string_pretty(m).unwrap_or_default())
}

/// A short room name from an ssh target: "me@vps.example.com" -> "vps".
pub fn server_name(ssh: &str) -> String {
    let host = ssh.rsplit('@').next().unwrap_or(ssh);
    let first = host.split('.').next().unwrap_or(host);
    if first.chars().all(|c| c.is_ascii_digit()) {
        host.to_string() // an IP address stays whole
    } else {
        first.to_string()
    }
}

/// A room on another machine: shown here, its agents run there.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct RemoteRoom {
    /// The server room's name (its entry in server-rooms.json).
    #[serde(alias = "machine")]
    pub server: String,
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
    /// The agent's own session (e.g. Claude's chat ID), so each tab continues its own chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
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
