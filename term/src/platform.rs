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
pub fn in_place() -> bool {
    if cfg!(target_os = "macos") {
        // the Mac stays on one desktop; the helper marks Lights Out as on
        return Path::new("/var/run/lightsout/active").exists();
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
    let mut found = None;
    for file in [PathBuf::from("/usr/local/share/lightsout/resume.conf"), home().join(".config/lightsout/resume.conf")] {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
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
        let _ = saved;
        mac_quit_apps();
        return linux_switch("out");
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
    let ok = Command::new("systemctl").arg("reboot").status().is_ok_and(|s| s.success());
    if !ok {
        let _ = Command::new("sudo").args(["systemctl", "reboot"]).status();
    }
}

/// Windows path -> path inside Lights Out (C:\DEV\x -> /mnt/c/DEV/x).
pub fn map_path(win: &str) -> PathBuf {
    let b = win.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        let drive = (b[0] as char).to_ascii_lowercase();
        let rest = win[2..].replace('\\', "/");
        return PathBuf::from(format!("/mnt/{drive}{rest}"));
    }
    PathBuf::from(win)
}

/// Tabs handed over from Windows (read once, then removed).
pub fn take_handoff() -> Option<crate::session::Saved> {
    let f = handoff_dir().join("tabs.json");
    let saved = crate::session::load(&f);
    let _ = std::fs::remove_file(f);
    saved
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
