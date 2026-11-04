//! Everything that differs between Windows and Lights Out (Linux).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The handover folder on the Windows drive (encrypted along with it).
pub fn lightsout_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\LightsOut")
    } else {
        PathBuf::from("/mnt/c/LightsOut")
    }
}

/// The small Linux started from Windows (Windows is off).
pub fn image() -> bool {
    cfg!(target_os = "linux") && Path::new("/mnt/c/LightsOut/system.img").exists()
}

/// On a Linux PC itself: the desktop is stopped and only Lights Out runs.
/// Mac: the helper chose to log out (the background service runs the agents),
/// rather than staying signed in (local servers would end with a log-out).
static MAC_LOGOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn mac_logging_out() -> bool {
    MAC_LOGOUT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Mac: back from Lights Out. Takes the tabs from the background service and
/// puts the Mac's settings back. Returns true when there was something to take.
pub fn mac_return(session: &str) -> bool {
    if !cfg!(target_os = "macos") || !in_place() {
        return false;
    }
    let had = crate::api::call(session, "handback", serde_json::json!({})).is_ok();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = Command::new("sudo").args(["-n", LINUX_SWITCH, "in"]).status();
    #[cfg(target_os = "macos")]
    mac_reopen_apps();
    had
}

pub fn in_place() -> bool {
    if cfg!(target_os = "macos") {
        // the Mac stays on one desktop; the helper marks Lights Out as on
        return Path::new("/Library/Application Support/LightsOut/active").exists();
    }
    cfg!(target_os = "linux") && std::env::var("LIGHTSOUT_MODE").is_ok_and(|m| m == "inplace")
}

/// Lights Out mode: only this terminal runs (Windows off, or the Linux desktop stopped).
pub fn in_lightsout() -> bool {
    image() || in_place()
}

/// Kept across restarts (sessions, handover). Linux: ~/.local/state/lightsout.
pub fn data_dir() -> PathBuf {
    let d = if cfg!(windows) {
        crate::api::state_dir()
    } else {
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/state")).join("lightsout")
    };
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Where tabs wait while switching.
fn handoff_dir() -> PathBuf {
    if cfg!(windows) || image() {
        lightsout_dir().join("handoff")
    } else {
        data_dir().join("handoff")
    }
}

/// Linux: the small root helper the Linux installer puts in place.
const LINUX_SWITCH: &str = "/usr/local/lib/lightsout/switch";

fn linux_switch(to: &str) -> Result<(), String> {
    match Command::new("sudo").args(["-n", LINUX_SWITCH, to]).output() {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("Lights Out isn't set up on this computer yet. Install it with: curl -fsSL https://lightsout.sh | sh".into()),
    }
}

/// Mac: quit every other app politely (apps with unsaved work stay open and
/// ask), remember them, and reopen them on Lights On.
#[cfg(target_os = "macos")]
fn mac_quit_apps() {
    let list = Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get name of every application process whose background only is false"])
        .output();
    let Ok(out) = list else { return };
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .trim()
        .split(", ")
        .map(str::to_string)
        .filter(|n| !n.is_empty() && !["Finder", "lightsout-app", "Lights Out", "Terminal", "iTerm2"].contains(&n.as_str()))
        .collect();
    let _ = std::fs::write(data_dir().join("quit-apps.txt"), names.join("\n"));
    for n in &names {
        let _ = Command::new("osascript").args(["-e", &format!("tell application \"{n}\" to quit")]).spawn();
    }
}

#[cfg(target_os = "macos")]
fn mac_reopen_apps() {
    let f = data_dir().join("quit-apps.txt");
    if let Ok(s) = std::fs::read_to_string(&f) {
        for n in s.lines().filter(|l| !l.is_empty()) {
            let _ = Command::new("open").args(["-g", "-a", n]).spawn();
        }
    }
    let _ = std::fs::remove_file(f);
}

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn on_path(exe: &str) -> bool {
    let sep = if cfg!(windows) { ';' } else { ':' };
    std::env::var("PATH").unwrap_or_default().split(sep).any(|d| {
        let p = Path::new(d).join(exe);
        p.exists() || (cfg!(windows) && p.with_extension("exe").exists())
    })
}

/// (program, args) for a plain shell tab.
pub fn shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        let ps = if on_path("pwsh.exe") { "pwsh.exe" } else { "powershell.exe" };
        (ps.into(), vec!["-NoLogo".into()])
    } else if image() {
        ("/bin/bash".into(), vec!["--rcfile".into(), BASHRC.into(), "-i".into()])
    } else {
        (user_shell(), vec!["-l".into(), "-i".into()])
    }
}

/// Your own shell on Linux and Mac (zsh, bash, fish...).
fn user_shell() -> String {
    std::env::var("SHELL").ok().filter(|s| Path::new(s).exists()).unwrap_or_else(|| "/bin/bash".into())
}

const BASHRC: &str = "/usr/local/lib/lightsout/bashrc";

/// (program, args) for an agent tab: the agent starts inside a shell, so the
/// shell is still there when the agent exits.
pub fn agent(cmd: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        let (ps, _) = shell();
        (ps, vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), cmd.into()])
    } else if image() {
        let rest = format!("{cmd}; exec bash --rcfile {BASHRC} -i");
        ("/bin/bash".into(), vec!["--rcfile".into(), BASHRC.into(), "-i".into(), "-c".into(), rest])
    } else {
        let sh = user_shell();
        let rest = format!("{cmd}; exec {sh} -l -i");
        (sh, vec!["-l".into(), "-i".into(), "-c".into(), rest])
    }
}

// ---------------------------------------------------------------- battery

pub struct Battery {
    pub percent: u8,
    pub watts: Option<f32>,
    pub charging: bool,
}

#[cfg(target_os = "linux")]
pub fn battery() -> Option<Battery> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for e in dir.flatten() {
        let p = e.path();
        if !e.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let read = |f: &str| std::fs::read_to_string(p.join(f)).ok().map(|s| s.trim().to_string());
        let percent = read("capacity")?.parse().ok()?;
        let watts = read("power_now")
            .and_then(|v| v.parse::<f32>().ok())
            .map(|uw| uw / 1e6)
            .or_else(|| {
                let c: f32 = read("current_now")?.parse().ok()?;
                let v: f32 = read("voltage_now")?.parse().ok()?;
                Some(c * v / 1e12)
            });
        let charging = read("status").map(|s| s == "Charging" || s == "Full").unwrap_or(false);
        return Some(Battery { percent, watts, charging });
    }
    None
}

#[cfg(windows)]
pub fn battery() -> Option<Battery> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 || s.BatteryLifePercent == 255 {
        return None;
    }
    Some(Battery { percent: s.BatteryLifePercent, watts: None, charging: s.ACLineStatus == 1 })
}

#[cfg(target_os = "macos")]
pub fn battery() -> Option<Battery> {
    // "-InternalBattery-0 (id=...)\t85%; discharging; 5:12 remaining present: true"
    let out = Command::new("pmset").args(["-g", "batt"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("InternalBattery"))?;
    let pct = line.split('\t').nth(1)?.split('%').next()?.trim().parse::<u8>().ok()?;
    let charging = line.contains("charging;") && !line.contains("discharging") || line.contains("charged");
    Some(Battery { percent: pct, watts: None, charging })
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn battery() -> Option<Battery> {
    None
}

// ---------------------------------------------------------------- screen

/// Turns the panel off/on (Lights Out). The compositor keeps reading keys,
/// so any key wakes it again.
pub fn screen(on: bool) {
    if cfg!(target_os = "macos") {
        let _ = if on { Command::new("caffeinate").args(["-u", "-t", "2"]).spawn().map(|_| ()) } else { Command::new("pmset").arg("displaysleepnow").status().map(|_| ()) };
        return;
    }
    if cfg!(target_os = "linux") {
        let ok = Command::new("wlopm").args([if on { "--on" } else { "--off" }, "*"]).status().is_ok_and(|s| s.success());
        let ok = ok
            || Command::new("swaymsg").args(["output", "*", "power", if on { "on" } else { "off" }]).status().is_ok_and(|s| s.success());
        if !ok {
            let _ = Command::new("brightnessctl").args(["-q", "set", if on { "50%" } else { "0" }]).status();
        }
    }
}

// ---------------------------------------------------------------- switching

/// How a program continues its last session, from plain data files:
/// /usr/local/share/lightsout/resume.conf, then ~/.config/lightsout/resume.conf
/// (lines "program = command"; the later file wins). None = not known.
pub fn resume_command(command: &str) -> Option<String> {
    let program = command.split_whitespace().next()?.rsplit(['/', '\\']).next()?.to_lowercase();
    let program = program.strip_suffix(".exe").or(program.strip_suffix(".cmd")).unwrap_or(&program).to_string();
    let mut found = None;
    let builtin = include_str!("../../os/overlay/usr/local/share/lightsout/resume.conf").to_string();
    let texts = [PathBuf::from("/usr/local/share/lightsout/resume.conf"), home().join(".config/lightsout/resume.conf")]
        .into_iter()
        .filter_map(|f| std::fs::read_to_string(f).ok());
    for text in std::iter::once(builtin).chain(texts) {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if let Some((p, c)) = line.split_once('=') {
                if p.trim().eq_ignore_ascii_case(&program) && !c.trim().is_empty() {
                    found = Some(c.trim().to_string());
                }
            }
        }
    }
    found
}

/// Windows: the command running inside each tab's shell (any program),
/// e.g. "claude", "npm run dev", "python manage.py runserver".
#[cfg(windows)]
pub fn running_in_tabs(shells: &[u32]) -> std::collections::HashMap<u32, Option<String>> {
    use std::collections::HashMap;
    #[derive(serde::Deserialize)]
    #[allow(non_snake_case)]
    struct P {
        ProcessId: u32,
        ParentProcessId: u32,
        Name: Option<String>,
        CommandLine: Option<String>,
    }
    let mut out = HashMap::new();
    let ps = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Compress",
        ])
        .output();
    let Ok(ps) = ps else { return out };
    let procs: Vec<P> = serde_json::from_slice(&ps.stdout).unwrap_or_default();
    for &shell in shells {
        // everything below the shell
        let mut all: Vec<&P> = Vec::new();
        let mut queue = vec![shell];
        while let Some(pid) = queue.pop() {
            for c in procs.iter().filter(|p| p.ParentProcessId == pid && p.ProcessId != pid) {
                let n = c.Name.clone().unwrap_or_default().to_lowercase();
                if n == "conhost.exe" || n == "openconsole.exe" {
                    continue;
                }
                all.push(c);
                queue.push(c.ProcessId);
            }
        }
        let command = all.first().and_then(|p| p.CommandLine.as_deref()).map(friendly_command);
        out.insert(shell, command.filter(|c| !c.is_empty()));
    }
    out
}

#[cfg(not(windows))]
pub fn running_in_tabs(_shells: &[u32]) -> std::collections::HashMap<u32, Option<String>> {
    Default::default()
}

/// "C:\Program Files\nodejs\node.exe" "...\npm-cli.js" run dev  ->  npm run dev
#[cfg_attr(not(windows), allow(dead_code))]
fn friendly_command(cmdline: &str) -> String {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in cmdline.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let base = |t: &str| {
        let b = t.rsplit(['\\', '/']).next().unwrap_or(t).to_string();
        let lower = b.to_lowercase();
        match lower.strip_suffix(".exe").or_else(|| lower.strip_suffix(".cmd")) {
            Some(stem) => stem.to_string(),
            None => b,
        }
    };
    // cmd /c <something>  ->  <something>
    if tokens.first().map(|t| base(t).eq_ignore_ascii_case("cmd")).unwrap_or(false) {
        if let Some(i) = tokens.iter().position(|t| t.eq_ignore_ascii_case("/c")) {
            return friendly_command(&tokens[i + 1..].join(" "));
        }
    }
    // node <path>\npm-cli.js args  ->  npm args (same for npx, pnpm, yarn)
    if tokens.len() > 1 && base(&tokens[0]).eq_ignore_ascii_case("node") {
        let script = base(&tokens[1]).to_lowercase();
        for (pat, tool) in [("npm-cli.js", "npm"), ("npx-cli.js", "npx"), ("pnpm.cjs", "pnpm"), ("yarn.js", "yarn")] {
            if script == pat {
                return std::iter::once(tool.to_string()).chain(tokens[2..].iter().cloned()).collect::<Vec<_>>().join(" ");
            }
        }
    }
    // node <...\\node_modules\\pkg\\...\\script.js> args: name it after the npm command that runs it
    if tokens.len() > 1 && base(&tokens[0]).eq_ignore_ascii_case("node") {
        if let Some(name) = npm_command_for(&tokens[1]) {
            return std::iter::once(name).chain(tokens[2..].iter().cloned()).collect::<Vec<_>>().join(" ");
        }
    }
    if let Some(first) = tokens.first_mut() {
        *first = base(first);
    }
    tokens.join(" ")
}

/// Which globally installed npm command runs this script? Looks through the
/// .cmd shims npm creates (e.g. %APPDATA%\\npm\\codex.cmd), so any npm CLI works.
#[cfg_attr(not(windows), allow(dead_code))]
fn npm_command_for(script: &str) -> Option<String> {
    let lower = script.to_lowercase().replace('/', "\\");
    let rel = &lower[lower.find("node_modules\\")?..];
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("npm");
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("cmd")).unwrap_or(false) {
            let text = std::fs::read_to_string(&p).unwrap_or_default().to_lowercase().replace('/', "\\");
            if text.contains(rel) {
                return p.file_stem().map(|s| s.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod wifi_tests {
    #[test]
    fn reads_iwd_list() {
        let text = "                               Available networks
--------------------------------------------------------------------------------
      Network name                      Security            Signal
--------------------------------------------------------------------------------
    > Home Wi-Fi                        psk                 ****
      Cafe                              open                **  
";
        let nets = super::parse_iwd(text);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "Home Wi-Fi");
        assert!(nets[0].connected && nets[0].secure && nets[0].signal == 4);
        assert!(!nets[1].secure && !nets[1].connected && nets[1].signal == 2);
    }
}

#[cfg(test)]
mod tests {
    use super::friendly_command;
    #[test]
    fn commands() {
        assert_eq!(friendly_command(r#""C:\Program Files\nodejs\node.exe" "C:\Program Files\nodejs\node_modules\npm\bin\npm-cli.js" run dev"#), "npm run dev");
        assert_eq!(friendly_command(r#"C:\WINDOWS\system32\cmd.exe /d /s /c "vitest run""#), "vitest run");
        assert_eq!(friendly_command(r#""C:\Python312\python.exe" manage.py runserver"#), "python manage.py runserver");
        assert_eq!(friendly_command(r#"C:\WINDOWS\system32\PING.EXE -n 30 127.0.0.1"#), "ping -n 30 127.0.0.1");
    }
}

fn random_hex() -> String {
    let mut b = [0u8; 32];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Windows -> Lights Out: save the tabs, leave a one-time "came from Windows"
/// pass (skips the lock screen), and ask the Lights Out task to restart.
pub fn enter_lightsout(saved: &crate::session::Saved) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Mac: the same window stays (agents never stop); everything else goes quiet
        // the tabs go to the background service that keeps agents running while
        // you're logged out; the helper waits for this app to close, starts it,
        // and logs you out (apps with unsaved work still ask first)
        save_handoff(saved);
        mac_quit_apps();
        let pid = std::process::id().to_string();
        return match Command::new("sudo").args(["-n", LINUX_SWITCH, "out", &pid]).output() {
            Ok(o) if o.status.success() => {
                let logout = String::from_utf8_lossy(&o.stdout).contains("logout");
                MAC_LOGOUT.store(logout, std::sync::atomic::Ordering::SeqCst);
                if !logout {
                    // staying signed in: this window keeps the agents, nothing to hand over
                    let _ = std::fs::remove_file(handoff_dir().join("tabs.json"));
                }
                Ok(())
            }
            _ => Err("Lights Out isn't set up on this Mac yet. Install it with: curl -fsSL https://lightsout.sh | sh".into()),
        };
    }
    #[allow(unreachable_code)]
    if cfg!(target_os = "linux") {
        // Linux PC: stop the desktop, start the Lights Out screen (no restart)
        let dir = handoff_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        crate::session::save(&dir.join("tabs.json"), saved);
        return linux_switch("out");
    }
    if !cfg!(windows) {
        return Err("Lights Out mode isn't available on this system yet.".into());
    }
    // a waiting Windows update would install during the restart, and Lights Out
    // must never open the drive in the middle of that
    for key in [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
    ] {
        if Command::new("reg").args(["query", key]).output().is_ok_and(|o| o.status.success()) {
            return Err("Windows has an update waiting. Restart Windows once to finish it, then press Lights Out again.".into());
        }
    }
    let dir = lightsout_dir().join("handoff");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    crate::session::save(&dir.join("tabs.json"), saved);
    std::fs::write(dir.join("nonce"), random_hex()).map_err(|e| e.to_string())?;
    let ok = Command::new("schtasks").args(["/run", "/tn", r"\LightsOut\Enter"]).output();
    match ok {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("Lights Out isn't installed on this PC yet. Install it with: irm https://lightsout.sh | iex".into()),
    }
}

/// Lights On: back to Windows (restart), or back to the Linux desktop (tabs come along).
pub fn back_to_windows(saved: &crate::session::Saved) {
    #[cfg(target_os = "macos")]
    {
        let _ = saved;
        let _ = linux_switch("in");
        mac_reopen_apps();
        return;
    }
    #[allow(unreachable_code)]
    if in_place() {
        let dir = handoff_dir();
        let _ = std::fs::create_dir_all(&dir);
        crate::session::save(&dir.join("tabs.json"), saved);
        let _ = linux_switch("in");
        return;
    }
    if image() {
        let dir = lightsout_dir().join("handoff");
        let _ = std::fs::create_dir_all(&dir);
        crate::session::save(&dir.join("back.json"), saved);
    }
    let ok = Command::new("systemctl").arg("reboot").status().is_ok_and(|s| s.success());
    if !ok {
        let _ = Command::new("sudo").args(["systemctl", "reboot"]).status();
    }
}

/// Windows path -> path inside Lights Out (C:\DEV\x -> /mnt/c/DEV/x).
pub fn map_path(win: &str) -> PathBuf {
    if cfg!(windows) {
        // back from Lights Out: /mnt/c/DEV/x -> C:\DEV\x
        let b = win.as_bytes();
        if win.starts_with("/mnt/") && b.len() >= 6 && b[5].is_ascii_alphabetic() && (b.len() == 6 || b[6] == b'/') {
            let drive = (b[5] as char).to_ascii_uppercase();
            return PathBuf::from(format!("{drive}:\\{}", win.get(7..).unwrap_or("").replace('/', "\\")));
        }
        return PathBuf::from(win);
    }
    let b = win.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        let drive = (b[0] as char).to_ascii_lowercase();
        let rest = win[2..].replace('\\', "/");
        return PathBuf::from(format!("/mnt/{drive}{rest}"));
    }
    PathBuf::from(win)
}

/// Lights Out: C: was opened read-only (Windows was hibernated or not shut down cleanly).
pub fn drive_read_only() -> bool {
    image()
        && std::fs::read_to_string("/proc/mounts")
            .is_ok_and(|m| m.lines().any(|l| l.split_whitespace().nth(1) == Some("/mnt/c") && l.split_whitespace().nth(3).is_some_and(|o| o.split(',').any(|x| x == "ro"))))
}

/// Saves tabs for the other side of a switch (headless service -> app).
pub fn save_handoff(saved: &crate::session::Saved) {
    let dir = handoff_dir();
    let _ = std::fs::create_dir_all(&dir);
    crate::session::save(&dir.join("tabs.json"), saved);
}

/// Windows: tabs coming back from Lights Out (read once, then removed).
pub fn take_handback() -> Option<crate::session::Saved> {
    if !cfg!(windows) {
        return None;
    }
    let f = lightsout_dir().join("handoff").join("back.json");
    let saved = crate::session::load(&f);
    let _ = std::fs::remove_file(f);
    saved
}

/// The waiting tabs, without removing them.
pub fn peek_handoff() -> Option<crate::session::Saved> {
    crate::session::load(&handoff_dir().join("tabs.json"))
}

/// Tabs handed over from Windows (read once, then removed).
pub fn take_handoff() -> Option<crate::session::Saved> {
    let f = handoff_dir().join("tabs.json");
    let saved = crate::session::load(&f);
    let _ = std::fs::remove_file(f);
    saved
}

// ---------------------------------------------------------------- Wi-Fi

pub struct Net {
    pub ssid: String,
    pub secure: bool,
    /// 0-4 bars
    pub signal: u8,
    pub connected: bool,
}

fn wifi_iface() -> Option<String> {
    std::fs::read_dir("/sys/class/net").ok()?.flatten().find(|e| e.path().join("wireless").exists()).map(|e| e.file_name().to_string_lossy().to_string())
}

/// Wi-Fi chooser: Windows, Mac and Linux (in Lights Out and on the desktop).
pub fn wifi_available() -> bool {
    if cfg!(target_os = "linux") {
        wifi_iface().is_some() && (image() || on_path("nmcli"))
    } else {
        cfg!(any(windows, target_os = "macos"))
    }
}

/// Nearby networks, connected first, then by signal. Takes a few seconds.
pub fn wifi_scan() -> Vec<Net> {
    #[cfg(windows)]
    let mut nets = win_scan();
    #[cfg(target_os = "macos")]
    let mut nets = mac_scan();
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut nets = if image() { iwd_scan() } else { nm_scan() };
    nets.retain(|n| !n.ssid.is_empty());
    nets.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
    let mut seen = std::collections::HashSet::new();
    nets.retain(|n| seen.insert(n.ssid.clone()));
    nets
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            for d in it.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux only (tested everywhere)
fn iwd_scan() -> Vec<Net> {
    let Some(dev) = wifi_iface() else { return Vec::new() };
    let _ = Command::new("iwctl").args(["station", &dev, "scan"]).output();
    std::thread::sleep(std::time::Duration::from_secs(3));
    let Ok(out) = Command::new("iwctl").args(["station", &dev, "get-networks"]).output() else { return Vec::new() };
    parse_iwd(&strip_ansi(&String::from_utf8_lossy(&out.stdout)))
}

/// Rows look like "  > Home Wi-Fi        psk        ****" (">" = connected).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux only (tested everywhere)
fn parse_iwd(text: &str) -> Vec<Net> {
    let mut nets = Vec::new();
    let mut dashes = 0;
    for line in text.lines() {
        if line.trim_start().starts_with("---") {
            dashes += 1;
            continue;
        }
        if dashes < 2 || line.trim().is_empty() {
            continue;
        }
        let connected = line.trim_start().starts_with('>');
        let mut toks: Vec<&str> = line.trim_start().trim_start_matches('>').split_whitespace().collect();
        let Some(stars) = toks.pop() else { continue };
        let Some(security) = toks.pop() else { continue };
        if toks.is_empty() {
            continue;
        }
        nets.push(Net { ssid: toks.join(" "), secure: security != "open", signal: stars.chars().filter(|c| *c == '*').count().min(4) as u8, connected });
    }
    nets
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux only (tested everywhere)
fn nm_scan() -> Vec<Net> {
    let Ok(out) = Command::new("nmcli").args(["-t", "-f", "IN-USE,SSID,SECURITY,SIGNAL", "dev", "wifi", "list", "--rescan", "yes"]).output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            // fields are split by ':'; a ':' inside a name is written '\:'
            let mut f = Vec::new();
            let mut cur = String::new();
            let mut esc = false;
            for c in l.chars() {
                match (esc, c) {
                    (true, _) => {
                        cur.push(c);
                        esc = false;
                    }
                    (false, '\\') => esc = true,
                    (false, ':') => f.push(std::mem::take(&mut cur)),
                    _ => cur.push(c),
                }
            }
            f.push(cur);
            let signal: u8 = f.get(3)?.parse().unwrap_or(0);
            Some(Net { ssid: f.get(1)?.clone(), secure: !f.get(2)?.is_empty(), signal: (signal / 25).min(4), connected: f[0] == "*" })
        })
        .collect()
}

// Windows: the system Wi-Fi interface (not netsh text, which depends on the language)
#[cfg(windows)]
fn win_scan() -> Vec<Net> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::WiFi::*;
    let mut nets = Vec::new();
    unsafe {
        let (mut ver, mut h): (u32, HANDLE) = (0, std::ptr::null_mut());
        if WlanOpenHandle(2, std::ptr::null(), &mut ver, &mut h) != 0 {
            return nets;
        }
        let mut ifs: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, std::ptr::null(), &mut ifs) == 0 && !ifs.is_null() {
            let n = (*ifs).dwNumberOfItems as usize;
            let first = std::ptr::addr_of!((*ifs).InterfaceInfo) as *const WLAN_INTERFACE_INFO;
            for i in 0..n {
                let guid = (*first.add(i)).InterfaceGuid;
                WlanScan(h, &guid, std::ptr::null(), std::ptr::null(), std::ptr::null());
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
            for i in 0..n {
                let guid = (*first.add(i)).InterfaceGuid;
                let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
                if WlanGetAvailableNetworkList(h, &guid, 0, std::ptr::null(), &mut list) != 0 || list.is_null() {
                    continue;
                }
                let count = (*list).dwNumberOfItems as usize;
                let nf = std::ptr::addr_of!((*list).Network) as *const WLAN_AVAILABLE_NETWORK;
                for j in 0..count {
                    let a = &*nf.add(j);
                    let len = (a.dot11Ssid.uSSIDLength as usize).min(32);
                    let ssid = String::from_utf8_lossy(&a.dot11Ssid.ucSSID[..len]).to_string();
                    nets.push(Net {
                        ssid,
                        secure: a.bSecurityEnabled != 0,
                        signal: (a.wlanSignalQuality / 25).min(4) as u8,
                        connected: a.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                    });
                }
                WlanFreeMemory(list as *const _);
            }
            WlanFreeMemory(ifs as *const _);
        }
        WlanCloseHandle(h, std::ptr::null());
    }
    nets
}

#[cfg(windows)]
fn win_join(ssid: &str, password: Option<&str>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;");
    // a saved Windows profile for it? then just connect
    let known = Command::new("netsh").args(["wlan", "connect", &format!("name={ssid}")]).creation_flags(NO_WINDOW).output();
    if password.is_none() && known.as_ref().is_ok_and(|o| o.status.success()) {
        return Ok(());
    }
    let security = match password {
        Some(p) => format!(
            "<authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption><sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
            esc(p)
        ),
        None => "<authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption>".into(),
    };
    let xml = format!(
        "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>{n}</name><SSIDConfig><SSID><name>{n}</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>auto</connectionMode><MSM><security>{security}</security></MSM></WLANProfile>",
        n = esc(ssid)
    );
    let f = std::env::temp_dir().join(format!("lightsout-wifi-{}.xml", random_hex()));
    std::fs::write(&f, xml).map_err(|e| e.to_string())?;
    let added = Command::new("netsh").args(["wlan", "add", "profile", &format!("filename={}", f.display()), "user=current"]).creation_flags(NO_WINDOW).output();
    let _ = std::fs::remove_file(&f); // the password never stays on disk
    if !added.is_ok_and(|o| o.status.success()) {
        return Err("Windows didn't accept the network".into());
    }
    match Command::new("netsh").args(["wlan", "connect", &format!("name={ssid}")]).creation_flags(NO_WINDOW).output() {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("couldn't join (wrong password?)".into()),
    }
}

// Mac: nearby names come from the system report (recent macOS may hide some)
#[cfg(target_os = "macos")]
fn mac_wifi_device() -> Option<String> {
    let out = Command::new("networksetup").arg("-listallhardwareports").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut wifi = false;
    for l in text.lines() {
        if l.starts_with("Hardware Port:") {
            wifi = l.contains("Wi-Fi") || l.contains("AirPort");
        } else if wifi && l.starts_with("Device:") {
            return Some(l.trim_start_matches("Device:").trim().to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn mac_scan() -> Vec<Net> {
    let Ok(out) = Command::new("system_profiler").args(["SPAirPortDataType", "-json"]).output() else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return Vec::new() };
    let mut nets = Vec::new();
    for card in v["SPAirPortDataType"].as_array().into_iter().flatten() {
        for i in card["spairport_airport_interfaces"].as_array().into_iter().flatten() {
            let net = |n: &serde_json::Value, connected: bool| {
                let ssid = n["_name"].as_str().unwrap_or("").to_string();
                let secure = n["spairport_security_mode"].as_str().is_some_and(|m| !m.contains("none"));
                let dbm: i32 = n["spairport_signal_noise"].as_str().and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).unwrap_or(-80);
                let signal = match dbm {
                    d if d >= -55 => 4,
                    d if d >= -65 => 3,
                    d if d >= -75 => 2,
                    _ => 1,
                };
                Net { ssid, secure, signal, connected }
            };
            if i["spairport_current_network_information"].is_object() {
                nets.push(net(&i["spairport_current_network_information"], true));
            }
            for n in i["spairport_airport_other_local_wireless_networks"].as_array().into_iter().flatten() {
                nets.push(net(n, false));
            }
        }
    }
    nets.retain(|n| !n.ssid.contains("redacted"));
    nets
}

/// Joins a network (remembered for next time).
pub fn wifi_join(ssid: &str, password: Option<&str>) -> Result<(), String> {
    #[cfg(windows)]
    return win_join(ssid, password);
    #[cfg(target_os = "macos")]
    {
        let dev = mac_wifi_device().ok_or("no Wi-Fi adapter")?;
        let mut c = Command::new("networksetup");
        c.args(["-setairportnetwork", &dev, ssid]);
        if let Some(p) = password {
            c.arg(p);
        }
        // networksetup reports problems as text, even on success codes
        return match c.output() {
            Ok(o) if o.status.success() && String::from_utf8_lossy(&o.stdout).trim().is_empty() => Ok(()),
            Ok(o) => Err(String::from_utf8_lossy(&o.stdout).trim().lines().last().unwrap_or("couldn't join").to_string()),
            Err(e) => Err(e.to_string()),
        };
    }
    #[allow(unreachable_code)]
    let out = if image() {
        let dev = wifi_iface().ok_or("no Wi-Fi adapter")?;
        let mut c = Command::new("iwctl");
        if let Some(p) = password {
            c.args(["--passphrase", p]);
        }
        c.args(["station", &dev, "connect", ssid]).output()
    } else {
        let mut c = Command::new("nmcli");
        c.args(["dev", "wifi", "connect", ssid]);
        if let Some(p) = password {
            c.args(["password", p]);
        }
        c.output()
    };
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let msg = strip_ansi(&String::from_utf8_lossy(&o.stdout)) + &String::from_utf8_lossy(&o.stderr);
            let msg = msg.trim();
            Err(if msg.is_empty() { "couldn't join (wrong password?)".into() } else { msg.lines().last().unwrap_or(msg).to_string() })
        }
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------- lid and sleep

/// While the app runs on a normal desktop, closing the lid never stops it:
/// Windows: lid action "do nothing" (your setting is put back by `lid_restore`);
/// Linux: a lid/sleep block that ends with the app.
pub fn lid_keep_running() {
    if in_lightsout() {
        return; // Lights Out handles the lid itself (screen off)
    }
    #[cfg(windows)]
    {
        let f = data_dir().join("lid-setting.txt");
        if !f.exists() {
            if let Some((ac, dc)) = lid_values() {
                let _ = std::fs::write(&f, format!("{ac} {dc}"));
            }
        }
        set_lid(0, 0);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = linux_switch("awake");
    }
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id().to_string();
        let _ = Command::new("systemd-inhibit")
            .args(["--what=handle-lid-switch:sleep", "--who=Lights Out", "--why=agents keep running", "--mode=block"])
            .args(["sh", "-c", &format!("while kill -0 {me} 2>/dev/null; do sleep 20; done")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Puts your own lid setting back, when the app closes.
pub fn lid_restore() {
    #[cfg(target_os = "macos")]
    {
        if !in_lightsout() {
            let _ = linux_switch("sleep");
        }
    }
    #[cfg(windows)]
    {
        let f = data_dir().join("lid-setting.txt");
        if let Ok(s) = std::fs::read_to_string(&f) {
            let v: Vec<u32> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            if let [ac, dc] = v[..] {
                set_lid(ac, dc);
            }
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(windows)]
fn lid_values() -> Option<(u32, u32)> {
    // output is localized; the last two hex numbers are the AC and DC values
    let out = Command::new("powercfg").args(["/qh", "SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let hex: Vec<u32> = text.split_whitespace().filter_map(|t| t.strip_prefix("0x")).filter_map(|h| u32::from_str_radix(h, 16).ok()).collect();
    match hex[..] {
        [.., ac, dc] => Some((ac, dc)),
        _ => None,
    }
}

#[cfg(windows)]
fn set_lid(ac: u32, dc: u32) {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    for (k, v) in [("/setacvalueindex", ac), ("/setdcvalueindex", dc)] {
        let _ = Command::new("powercfg").args([k, "SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION", &v.to_string()]).creation_flags(NO_WINDOW).status();
    }
    let _ = Command::new("powercfg").args(["/setactive", "SCHEME_CURRENT"]).creation_flags(NO_WINDOW).status();
}

/// Windows: no sleeping while a program runs in a tab (an agent at work).
pub fn stay_awake(on: bool) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
        SetThreadExecutionState(if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS });
    }
    let _ = on;
}

// ---------------------------------------------------------------- what runs in a shell

/// For each shell PID: the program running inside it (None = the shell is idle).
/// Cheap enough to call every couple of seconds.
pub fn running_programs(shells: &[u32]) -> std::collections::HashMap<u32, Option<String>> {
    let mut out = std::collections::HashMap::new();
    let children = process_children();
    let wrappers = ["cmd", "conhost", "openconsole", "sh", "bash", "dash", "zsh", "fish", "login", "env", "sudo"];
    for &shell in shells {
        // follow wrapper processes (e.g. cmd.exe running an npm .cmd shim) to the real program
        let mut pid = shell;
        let mut name = None;
        for _ in 0..4 {
            let Some(kids) = children.get(&pid) else { break };
            let Some((kid, kname)) = kids.iter().find(|(_, n)| n != "conhost" && n != "openconsole") else { break };
            name = Some(kname.clone());
            pid = *kid;
            if !wrappers.contains(&kname.as_str()) {
                break;
            }
        }
        out.insert(shell, name);
    }
    out
}

/// parent PID -> [(child PID, program name without .exe, lowercase)]
#[cfg(windows)]
fn process_children() -> std::collections::HashMap<u32, Vec<(u32, String)>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut map: std::collections::HashMap<u32, Vec<(u32, String)>> = std::collections::HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            let name = name.strip_suffix(".exe").unwrap_or(&name).to_string();
            map.entry(e.th32ParentProcessID).or_default().push((e.th32ProcessID, name));
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    map
}

#[cfg(target_os = "macos")]
fn process_children() -> std::collections::HashMap<u32, Vec<(u32, String)>> {
    let mut map: std::collections::HashMap<u32, Vec<(u32, String)>> = std::collections::HashMap::new();
    if let Ok(out) = Command::new("ps").args(["-axo", "pid=,ppid=,comm="]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut it = line.split_whitespace();
            let (Some(pid), Some(ppid)) = (it.next().and_then(|p| p.parse().ok()), it.next().and_then(|p| p.parse::<u32>().ok())) else { continue };
            let comm: String = it.collect::<Vec<_>>().join(" ");
            let name = comm.rsplit('/').next().unwrap_or("").trim_start_matches('-').to_lowercase();
            map.entry(ppid).or_default().push((pid, name));
        }
    }
    map
}

#[cfg(not(any(windows, target_os = "macos")))]
fn process_children() -> std::collections::HashMap<u32, Vec<(u32, String)>> {
    let mut map: std::collections::HashMap<u32, Vec<(u32, String)>> = std::collections::HashMap::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
            // "pid (name) state ppid ..."
            let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else { continue };
            let name = stat[a + 1..b].to_lowercase();
            let ppid = stat[b + 2..].split_whitespace().nth(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            map.entry(ppid).or_default().push((pid, name));
        }
    }
    map
}

// ---------------------------------------------------------------- alerts, editor, console

/// A short sound (Windows; Lights Out stays silent to save power).
pub fn sound(file: &str, needs_you: bool) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        unsafe {
            if !file.is_empty() && Path::new(file).exists() {
                PlaySoundW(wide(file).as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_ASYNC | SND_NODEFAULT);
            } else {
                let alias = if needs_you { "SystemExclamation" } else { "SystemAsterisk" };
                PlaySoundW(wide(alias).as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let f = if !file.is_empty() && Path::new(file).exists() {
            file.to_string()
        } else {
            format!("/System/Library/Sounds/{}.aiff", if needs_you { "Glass" } else { "Tink" })
        };
        let _ = Command::new("afplay").arg(f).spawn();
    }
    let _ = (file, needs_you);
}

/// A system notification when the window isn't in front (Windows).
pub fn os_notify(text: &str) {
    #[cfg(windows)]
    {
        let _ = notify_rust::Notification::new().summary("Lights Out").body(text).show();
    }
    #[cfg(target_os = "macos")]
    {
        let t = text.replace('"', "'");
        let _ = Command::new("osascript").args(["-e", &format!("display notification \"{t}\" with title \"Lights Out\"")]).spawn();
    }
    let _ = text;
}

/// Opens a text file in an editor. Windows: Notepad. Lights Out: returns the
/// command to run in a new tab (there is no desktop to open a window on).
pub fn editor_command(file: &Path) -> Option<String> {
    if cfg!(windows) {
        let _ = Command::new("notepad.exe").arg(file).spawn();
        None
    } else {
        let ed = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
        Some(format!("{ed} '{}'", file.display()))
    }
}

/// Lets `lightsout <command>` print to the terminal it was started from
/// (the app itself is a windowed program on Windows).
pub fn attach_console() {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}

/// Claude Code keys its history by folder path. Point the Linux path's key at
/// the Windows one, so `claude --continue` picks up the same conversation.
pub fn link_claude_history(win_cwd: &str, linux_cwd: &Path) {
    if cfg!(windows) {
        return; // only Lights Out links its folder names to the Windows ones
    }
    let key = |p: &str| p.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>();
    let projects = home().join(".claude").join("projects");
    let win = projects.join(key(win_cwd));
    let lin = projects.join(key(&linux_cwd.to_string_lossy()));
    if win.is_dir() && !lin.exists() {
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(&win, &lin);
    }
}

// ---------------------------------------------------------------- lock screen

pub enum Lock {
    Unlocked,
    Locked { salt: Vec<u8>, hash: String, wrong: u32 },
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

/// Lights Out starts locked, unless it was entered from Windows a moment ago
/// (the one-time pass on the encrypted drive matches the one on the EFI partition).
pub fn initial_lock() -> Lock {
    if !in_lightsout() {
        return Lock::Unlocked;
    }
    let c_nonce = lightsout_dir().join("handoff").join("nonce");
    let esp_nonce = PathBuf::from("/boot/efi/EFI/lightsout/handoff.nonce");
    let a = std::fs::read_to_string(&c_nonce).unwrap_or_default();
    let b = std::fs::read_to_string(&esp_nonce).unwrap_or_default();
    let fresh = std::fs::metadata(&c_nonce)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().map(|e| e.as_secs() < 900).unwrap_or(false))
        .unwrap_or(false);
    let _ = std::fs::remove_file(&c_nonce);
    let _ = Command::new("sudo").args(["rm", "-f", &esp_nonce.to_string_lossy()]).status();
    if !a.trim().is_empty() && a.trim() == b.trim() && fresh {
        return Lock::Unlocked;
    }
    match std::fs::read_to_string(lightsout_dir().join("config").join("pin")) {
        Ok(s) => {
            let mut parts = s.trim().splitn(2, ':');
            let salt = unhex(parts.next().unwrap_or(""));
            let hash = parts.next().unwrap_or("").to_lowercase();
            if hash.is_empty() {
                Lock::Unlocked
            } else {
                Lock::Locked { salt, hash, wrong: 0 }
            }
        }
        Err(_) => Lock::Unlocked, // no PIN set up (developer machines)
    }
}

pub fn pin_matches(salt: &[u8], hash: &str, pin: &str) -> bool {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(pin.as_bytes());
    let got: String = h.finalize().iter().map(|x| format!("{x:02x}")).collect();
    got == hash
}

#[cfg(all(test, windows))]
mod switch_tests {
    use std::path::PathBuf;

    #[test]
    fn paths_come_back_to_windows() {
        assert_eq!(super::map_path("/mnt/c/DEV/my app"), PathBuf::from(r"C:\DEV\my app"));
        assert_eq!(super::map_path("/mnt/d"), PathBuf::from(r"D:\"));
        assert_eq!(super::map_path(r"C:\x"), PathBuf::from(r"C:\x"));
    }

    #[test]
    fn known_agents_continue() {
        assert_eq!(super::resume_command("claude").as_deref(), Some("claude --continue"));
        assert_eq!(super::resume_command("codex.cmd").as_deref(), Some("codex resume --last"));
        assert_eq!(super::resume_command("npm run dev"), None);
    }
}

#[cfg(test)]
mod wifi_live {
    /// Real scan on this machine: `cargo test --release -- --ignored wifi_scan_here --nocapture`
    #[test]
    #[ignore]
    fn wifi_scan_here() {
        let nets = super::wifi_scan();
        println!("networks found: {}", nets.len());
        for n in &nets {
            println!("  signal {} secure {} connected {}", n.signal, n.secure, n.connected);
        }
        assert!(!nets.is_empty());
    }
}
