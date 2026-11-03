//! Everything Lights Out changes is written down before it changes it, so a
//! crash, a dead battery or a closed terminal can always be undone with
//! `lightsout restore`.

use crate::power::{self, Setting};
use crate::{procs, radio};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

pub const ALL: [Setting; 5] = [power::LID, power::TURBO, power::CPU_MAX, power::SAVER, power::SCREEN_IDLE];

pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("lightsout");
    let _ = fs::create_dir_all(d.join("agents"));
    let _ = fs::create_dir_all(d.join("logs"));
    d
}

fn file() -> PathBuf {
    dir().join("state.txt")
}

pub fn wake_flag() -> PathBuf {
    dir().join("wake")
}

#[derive(Default)]
pub struct Saved {
    pub settings: Vec<(Setting, (u32, u32))>,
    pub bluetooth_was_on: bool,
    pub efficiency: HashSet<u32>,
}

impl Saved {
    pub fn exists() -> bool {
        file().exists()
    }

    pub fn save(&self) {
        let mut s = String::new();
        for (st, (ac, dc)) in &self.settings {
            s += &format!("setting {} {} {} {}\n", st.group, st.name, ac, dc);
        }
        if self.bluetooth_was_on {
            s += "bluetooth on\n";
        }
        for pid in &self.efficiency {
            s += &format!("efficiency {pid}\n");
        }
        let _ = fs::write(file(), s);
    }

    pub fn load() -> Option<Saved> {
        let text = fs::read_to_string(file()).ok()?;
        let mut saved = Saved::default();
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["setting", g, n, ac, dc] => {
                    if let (Some(st), Ok(ac), Ok(dc)) =
                        (ALL.iter().find(|s| s.group == *g && s.name == *n), ac.parse(), dc.parse())
                    {
                        saved.settings.push((*st, (ac, dc)));
                    }
                }
                ["bluetooth", "on"] => saved.bluetooth_was_on = true,
                ["efficiency", pid] => {
                    if let Ok(p) = pid.parse() {
                        saved.efficiency.insert(p);
                    }
                }
                _ => {}
            }
        }
        Some(saved)
    }

    /// Puts everything back the way it was and forgets the saved state.
    pub fn restore(&self) {
        for (st, (ac, dc)) in &self.settings {
            power::write(*st, *ac, *dc);
        }
        power::activate();
        if self.bluetooth_was_on {
            radio::bluetooth_on();
        }
        for &pid in &self.efficiency {
            procs::set_efficiency(pid, false);
        }
        power::keep_awake(false);
        let _ = fs::remove_file(file());
    }
}

/// PIDs started with `lightsout run` that are still running.
pub fn agents() -> Vec<u32> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir().join("agents")) {
        for e in entries.flatten() {
            match e.file_name().to_string_lossy().parse::<u32>() {
                Ok(pid) if procs::alive(pid) => out.push(pid),
                _ => {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
    out
}

pub fn add_agent(pid: u32, command: &str) {
    let _ = fs::write(dir().join("agents").join(pid.to_string()), command);
}

pub fn remove_agent(pid: u32) {
    let _ = fs::remove_file(dir().join("agents").join(pid.to_string()));
}
