//! Plugins: a folder with `lightsout-plugin.toml` and programs in any language. A plugin adds
//! commands (to the one command list, so the menu, keys, `lightsout do` and the API reach
//! them), keys, and programs that run when something happens (events). It talks back
//! through the `lightsout` command line, like any agent in a pane.
//!
//! Where things are (all under ~/.config/lightsout, which Lights Out mode shares with
//! Windows, so a plugin works on both sides):
//! - `plugins/ID/`: an installed plugin's files
//! - `plugins.json`: which plugins there are, installed (from where) or linked (a folder you
//!   work on), and whether each is on
//! - `plugin-settings/ID/`: your settings for a plugin (kept when it is updated or removed)
//!
//! A plugin's own files (state, its log) go to Lights Out's data folder: `plugin-data/ID/`.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Every event a plugin can run on, and what it gets (LIGHTSOUT_EVENT_JSON).
pub const EVENTS: &[(&str, &str)] = &[
    ("app.started", "Lights Out started (tabs already restored)"),
    ("pane.opened", "a pane opened: pane, command, cwd"),
    ("pane.closed", "a pane closed: pane"),
    (
        "program.started",
        "a program started in a pane: pane, program",
    ),
    (
        "program.ended",
        "the program in a pane ended: pane, program",
    ),
    ("agent.needs-you", "an agent asks something: pane, title"),
    ("agent.done", "an agent finished: pane, title"),
    ("room.opened", "a room was made: room"),
    ("room.renamed", "a room got a new name: room, before"),
    ("room.closed", "a room was closed: room"),
    ("lights.out", "Lights Out is starting (tabs saved)"),
    ("lights.on", "Lights On: going back (tabs saved)"),
    ("screen.off", "the screen was switched off"),
    ("battery.low", "the battery is low: percent"),
    (
        "part.resized",
        "a part has a new size: part, cols, rows, width, height (pixels)",
    ),
    (
        "part.clicked",
        "a part was clicked (not on a command): part, x, y (pixels), col, row, button",
    ),
    (
        "part.scrolled",
        "the wheel turned over a part: part, lines (up > 0), x, y",
    ),
];

/// Names a plugin can't take: the first part of the built-in commands.
const RESERVED: &[&str] = &[
    "actions", "agent", "app", "command", "edit", "font", "lights", "pane", "plugin", "room",
    "run", "screen", "settings", "sidebar", "tab", "wifi",
];

#[derive(Deserialize, Clone, Debug)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub min_lightsout: String,
    #[serde(default)]
    pub description: String,
    /// windows, linux, macos; empty = all
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub keys: Vec<PluginKey>,
    #[serde(default)]
    pub events: Vec<PluginEvent>,
    #[serde(default)]
    pub links: Vec<PluginLink>,
    #[serde(default)]
    pub parts: Vec<PluginPart>,
}

/// A part of the screen a plugin draws: a strip on one side (any number, on any side),
/// filled with what its program sends (`lightsout part set`).
#[derive(Deserialize, Clone, Debug)]
pub struct PluginPart {
    pub id: String,
    /// left, right, top, bottom
    pub side: String,
    /// Columns (left, right) or rows (top, bottom).
    #[serde(default = "part_size")]
    pub size: u16,
    /// "sidebar" or "tabs": the built-in one goes, this one takes its place.
    #[serde(default)]
    pub replaces: String,
    /// "always" (default), or "when-filled": it takes room only while it shows something.
    #[serde(default = "always")]
    pub show: String,
    /// Started when the part appears (Lights Out starts, the plugin is added): it fills the
    /// part, and keeps it current from events.
    #[serde(default)]
    pub run: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

fn part_size() -> u16 {
    1
}

fn always() -> String {
    "always".into()
}

pub const SIDES: &[&str] = &["left", "right", "top", "bottom"];

/// Where a command's program runs: unseen in the background, or on screen in a pane.
pub const PLACES: &[&str] = &["background", "popup", "tab", "split", "split-down"];

#[derive(Deserialize, Clone, Debug)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    pub run: Vec<String>,
    /// background (default), popup (closes when the program ends), tab, split, split-down
    #[serde(default = "background")]
    pub place: String,
    /// Shown in the actions menu (default yes).
    #[serde(default = "yes")]
    pub menu: bool,
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Ctrl+click on a link that matches `pattern` (`*` = anything) runs `command` with the link
/// after it, instead of opening it in the browser.
#[derive(Deserialize, Clone, Debug)]
pub struct PluginLink {
    pub pattern: String,
    pub command: String,
}

fn background() -> String {
    "background".into()
}

#[derive(Deserialize, Clone, Debug)]
pub struct PluginKey {
    pub key: String,
    pub command: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PluginEvent {
    pub on: String,
    pub run: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

fn yes() -> bool {
    true
}

/// A plugin as Lights Out has it: its description and where its files are.
#[derive(Clone, Debug)]
pub struct Plugin {
    pub m: Manifest,
    pub root: PathBuf,
}

/// One line of plugins.json.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    /// owner/repo[/folder] it was installed from (none: linked)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    /// The folder of a linked plugin (none: installed, in plugins/ID)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

pub fn config_dir() -> PathBuf {
    crate::platform::home().join(".config").join("lightsout")
}

fn registry_file() -> PathBuf {
    config_dir().join("plugins.json")
}

pub fn installed_dir(id: &str) -> PathBuf {
    config_dir().join("plugins").join(id)
}

pub fn settings_dir(id: &str) -> PathBuf {
    config_dir().join("plugin-settings").join(id)
}

pub fn data_dir(id: &str) -> PathBuf {
    crate::platform::data_dir().join("plugin-data").join(id)
}

pub fn registry() -> Vec<Entry> {
    std::fs::read_to_string(registry_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_registry(v: &[Entry]) -> std::io::Result<()> {
    let f = registry_file();
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(f, serde_json::to_string_pretty(v).unwrap_or_default())
}

/// When plugins.json and each plugin's lightsout-plugin.toml last changed: when this
/// differs from before, the app reads its plugins again.
pub fn stamp() -> Vec<Option<std::time::SystemTime>> {
    let changed = |f: PathBuf| std::fs::metadata(f).and_then(|m| m.modified()).ok();
    std::iter::once(changed(registry_file()))
        .chain(
            registry()
                .iter()
                .map(|e| changed(root_of(e).join("lightsout-plugin.toml"))),
        )
        .collect()
}

fn this_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Runs here: no list, or this system named in it.
pub fn runs_here(platforms: &[String]) -> bool {
    platforms.is_empty() || platforms.iter().any(|p| p == this_platform())
}

/// `0.4.0` ≤ `0.4.1-rc.2`? (the parts before any `-`, number by number)
fn at_least(have: &str, need: &str) -> bool {
    let nums = |v: &str| -> Vec<u64> {
        v.split('-')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|n| n.parse().unwrap_or(0))
            .collect()
    };
    nums(have) >= nums(need)
}

/// A plugin's or command's name: lowercase letters, digits, - and _ (a dot separates them).
fn good_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Reads and checks a plugin's description.
pub fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let file = root.join("lightsout-plugin.toml");
    let text = std::fs::read_to_string(&file)
        .map_err(|_| format!("no lightsout-plugin.toml in {}", root.display()))?;
    let m: Manifest =
        toml::from_str(&text).map_err(|e| format!("lightsout-plugin.toml: {}", e.message()))?;
    if !good_name(&m.id) {
        return Err(format!(
            "plugin id `{}`: use lowercase letters, digits, - and _",
            m.id
        ));
    }
    if RESERVED.contains(&m.id.as_str()) {
        return Err(format!("plugin id `{}` is a name Lights Out uses", m.id));
    }
    if !m.min_lightsout.is_empty() && !at_least(env!("CARGO_PKG_VERSION"), &m.min_lightsout) {
        return Err(format!(
            "{} needs Lights Out {} or newer (this is {}): lightsout update",
            m.name,
            m.min_lightsout,
            env!("CARGO_PKG_VERSION")
        ));
    }
    for c in &m.commands {
        if !good_name(&c.id) {
            return Err(format!(
                "command `{}`: use lowercase letters, digits, - and _",
                c.id
            ));
        }
        if c.run.is_empty() {
            return Err(format!("command `{}` has nothing to run", c.id));
        }
        if !PLACES.contains(&c.place.as_str()) {
            return Err(format!(
                "command `{}`: place `{}` doesn't exist ({})",
                c.id,
                c.place,
                PLACES.join(", ")
            ));
        }
    }
    for l in &m.links {
        if l.pattern.is_empty() || l.command.trim().is_empty() {
            return Err("a link needs a pattern and a command".into());
        }
    }
    for p in &m.parts {
        if !good_name(&p.id) {
            return Err(format!(
                "part `{}`: use lowercase letters, digits, - and _",
                p.id
            ));
        }
        if !SIDES.contains(&p.side.as_str()) {
            return Err(format!(
                "part `{}`: side `{}` doesn't exist ({})",
                p.id,
                p.side,
                SIDES.join(", ")
            ));
        }
        if !["", "sidebar", "tabs"].contains(&p.replaces.as_str()) {
            return Err(format!(
                "part `{}`: it can replace sidebar or tabs, not `{}`",
                p.id, p.replaces
            ));
        }
        if !["always", "when-filled"].contains(&p.show.as_str()) {
            return Err(format!(
                "part `{}`: show is always or when-filled, not `{}`",
                p.id, p.show
            ));
        }
        if p.size == 0 || p.size > 200 {
            return Err(format!("part `{}`: size goes from 1 to 200", p.id));
        }
    }
    for e in &m.events {
        if !EVENTS.iter().any(|(n, _)| *n == e.on) {
            return Err(format!(
                "event `{}` doesn't exist (events: {})",
                e.on,
                EVENTS.iter().map(|e| e.0).collect::<Vec<_>>().join(", ")
            ));
        }
        if e.run.is_empty() {
            return Err(format!("the `{}` event has nothing to run", e.on));
        }
    }
    Ok(m)
}

/// The folder a plugin's files are in.
pub fn root_of(e: &Entry) -> PathBuf {
    match &e.path {
        Some(p) => PathBuf::from(p),
        None => installed_dir(&e.id),
    }
}

/// The plugins that are on and run on this system, and what is wrong with the others.
pub fn load() -> (Vec<Plugin>, Vec<String>) {
    let (mut ok, mut problems) = (Vec::new(), Vec::new());
    for e in registry().into_iter().filter(|e| e.enabled) {
        let root = root_of(&e);
        match read_manifest(&root) {
            Ok(m) if !runs_here(&m.platforms) => {}
            Ok(m) => ok.push(Plugin { m, root }),
            Err(err) => problems.push(format!("plugin {}: {err}", e.id)),
        }
    }
    (ok, problems)
}

/// A program's words as they run: `./x` and `../x` are in the plugin's folder (so they work
/// wherever the program runs, also in a pane in your own folder).
pub fn argv(p: &Plugin, run: &[String]) -> Vec<String> {
    run.iter()
        .map(|w| {
            if w.starts_with("./") || w.starts_with("../") {
                p.root.join(w).to_string_lossy().to_string()
            } else {
                w.clone()
            }
        })
        .collect()
}

/// What every program of a plugin gets: which plugin, its folder, its settings and its own
/// files (both made if missing).
pub fn env(p: &Plugin) -> Vec<(String, String)> {
    let (data, settings) = (data_dir(&p.m.id), settings_dir(&p.m.id));
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::create_dir_all(&settings);
    let s = |p: &Path| p.to_string_lossy().to_string();
    vec![
        ("LIGHTSOUT".into(), "1".into()),
        ("LIGHTSOUT_PLUGIN_ID".into(), p.m.id.clone()),
        ("LIGHTSOUT_PLUGIN_ROOT".into(), s(&p.root)),
        ("LIGHTSOUT_PLUGIN_CONFIG".into(), s(&settings)),
        ("LIGHTSOUT_PLUGIN_STATE".into(), s(&data)),
    ]
}

/// `*` stands for anything (also nothing); the rest must match as written.
pub fn matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// Runs one of a plugin's programs, in its folder, without waiting. What it prints goes to
/// its log (plugin-data/ID/log.txt). `extra` is added to its environment.
pub fn spawn(p: &Plugin, run: &[String], extra: &[(&str, String)]) -> Result<(), String> {
    let run = argv(p, run);
    let Some(program) = run.first().cloned() else {
        return Err("nothing to run".into());
    };
    let base = env(p); // (makes its folders)
    let log_file = data_dir(&p.m.id).join("log.txt");
    // a log that grew big starts again
    if std::fs::metadata(&log_file).is_ok_and(|m| m.len() > 1_000_000) {
        let _ = std::fs::remove_file(&log_file);
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut c = crate::platform::quiet(&program);
    c.args(&run[1..])
        .current_dir(&p.root)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err);
    for (k, v) in base {
        c.env(k, v);
    }
    for (k, v) in extra {
        c.env(k, v);
    }
    let mut child = c
        .spawn()
        .map_err(|e| format!("{}: could not start {program}: {e}", p.m.id))?;
    // collected when it ends, so it never lingers as a finished process
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

// ---------------------------------------------------------------- `lightsout plugin ...`

const HELP: &str = "\
lightsout plugin install OWNER/REPO[/FOLDER] [--ref TAG] [--yes]
                                  a plugin from GitHub (shows what it runs, then asks)
lightsout plugin link FOLDER      a plugin you are making (used where it is)
lightsout plugin list             your plugins
lightsout plugin update [ID]      the newest version of installed plugins
lightsout plugin remove ID        take one out (its settings stay)
lightsout plugin on ID | off ID   switch one on or off
lightsout plugin events           what a plugin can run on
";

/// `lightsout plugin ...`; answers the exit code.
pub fn cli(session: &str, args: &[String]) -> i32 {
    let flag = |n: &str| args.iter().any(|a| a == n);
    let value = |n: &str| {
        args.iter()
            .position(|a| a == n)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let words: Vec<&String> = {
        let mut v = Vec::new();
        let mut skip = false;
        for a in args.iter().skip(1) {
            if skip {
                skip = false;
            } else if a == "--ref" {
                skip = true;
            } else if !a.starts_with("--") {
                v.push(a);
            }
        }
        v
    };
    let code = match args.first().map(String::as_str) {
        Some("install") => match words.first() {
            Some(src) => install(src, value("--ref"), flag("--yes")),
            None => {
                eprint!("{HELP}");
                2
            }
        },
        Some("link") => match words.first() {
            Some(dir) => link(dir),
            None => {
                eprint!("{HELP}");
                2
            }
        },
        Some("list") | None => list(),
        Some("update") => update(words.first().map(|s| s.as_str()), flag("--yes")),
        Some("remove") | Some("uninstall") => match words.first() {
            Some(id) => remove(id),
            None => {
                eprint!("{HELP}");
                2
            }
        },
        Some(w @ ("on" | "off" | "enable" | "disable")) => match words.first() {
            Some(id) => switch(id, matches!(w, "on" | "enable")),
            None => {
                eprint!("{HELP}");
                2
            }
        },
        Some("events") => {
            for (n, what) in EVENTS {
                println!("{n:<18} {what}");
            }
            0
        }
        _ => {
            eprint!("{HELP}");
            2
        }
    };
    // the running app picks the change up now (it also notices on its own)
    if code == 0 {
        let _ = crate::api::call(session, "plugins-reload", json!({}));
    }
    code
}

fn list() -> i32 {
    let reg = registry();
    if reg.is_empty() {
        println!("No plugins yet. `lightsout plugin install OWNER/REPO` adds one.");
        return 0;
    }
    println!("{:<20} {:<10} {:<5} FROM", "PLUGIN", "VERSION", "ON");
    for e in reg {
        let version = read_manifest(&root_of(&e))
            .map(|m| m.version)
            .unwrap_or_else(|_| "broken".into());
        let from = match (&e.source, &e.path) {
            (Some(s), _) => format!(
                "{s}{}",
                e.r#ref
                    .as_ref()
                    .map(|r| format!(" @ {r}"))
                    .unwrap_or_default()
            ),
            (None, Some(p)) => format!("{p} (linked)"),
            _ => String::new(),
        };
        println!(
            "{:<20} {:<10} {:<5} {from}",
            e.id,
            version,
            if e.enabled { "yes" } else { "no" }
        );
    }
    0
}

/// A plugin's keys that are already used (by your settings, another plugin that is on, or
/// Lights Out): it isn't added then, and each is said with who has it and how to fix it.
/// True when there was one.
fn refuse_key_clashes(m: &Manifest) -> bool {
    let own: Vec<(String, String)> = crate::config::load()
        .config
        .keys
        .iter()
        .map(|(k, b)| (k.clone(), b.command()))
        .collect();
    let others: Vec<(String, String, String)> = load()
        .0
        .iter()
        .filter(|p| p.m.id != m.id)
        .flat_map(|p| {
            p.m.keys
                .iter()
                .map(|k| (p.m.id.clone(), k.key.clone(), k.command.clone()))
        })
        .collect();
    let keys: Vec<(String, String)> = m
        .keys
        .iter()
        .map(|k| (k.key.clone(), k.command.clone()))
        .collect();
    let clashes = crate::commands::clashes(&own, &others, &m.id, &keys);
    for c in &clashes {
        eprintln!("lightsout: {c}");
    }
    !clashes.is_empty()
}

/// Everything a plugin can start, for the person installing it to read.
fn describe(m: &Manifest, root: &Path) -> String {
    let mut s = format!("{} {} ({})\n", m.name, m.version, m.id);
    if !m.description.is_empty() {
        s += &format!("  {}\n", m.description);
    }
    s += &format!("  files: {}\n", root.display());
    s += "  It runs as you, like any program you install, and can use every Lights Out command.\n";
    s += "  What it starts:\n";
    for c in &m.commands {
        s += &format!(
            "    command {:<22} {}\n",
            format!("{}.{}", m.id, c.id),
            c.run.join(" ")
        );
    }
    for e in &m.events {
        s += &format!("    on {:<27} {}\n", e.on, e.run.join(" "));
    }
    for k in &m.keys {
        s += &format!("    key {:<26} {}\n", k.key, k.command);
    }
    for l in &m.links {
        s += &format!("    link {:<25} {}\n", l.pattern, l.command);
    }
    for p in &m.parts {
        let what = format!(
            "{} {}{}",
            p.side,
            p.size,
            if p.replaces.is_empty() {
                String::new()
            } else {
                format!(", instead of the {}", p.replaces)
            }
        );
        s += &format!("    part {:<25} {} ({what})\n", p.id, p.run.join(" "));
    }
    if m.commands.is_empty() && m.events.is_empty() && m.parts.is_empty() {
        s += "    nothing\n";
    }
    s
}

/// Each program a plugin can start, to see whether an update adds one.
fn programs(m: &Manifest) -> Vec<String> {
    m.commands
        .iter()
        .map(|c| c.run.join(" "))
        .chain(m.events.iter().map(|e| e.run.join(" ")))
        .chain(m.parts.iter().map(|p| p.run.join(" ")))
        .filter(|r| !r.is_empty())
        .collect()
}

fn ask(question: &str) -> bool {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        eprintln!("(not asked: no terminal; add --yes to install without asking)");
        return false;
    }
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    matches!(line.trim(), "y" | "Y" | "yes" | "Yes")
}

/// owner/repo/folder → (https://github.com/owner/repo, folder)
fn github(source: &str) -> Option<(String, String)> {
    let s = source
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');
    let mut parts = s.splitn(3, '/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let ok = |w: &str| {
        !w.is_empty()
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    if !ok(owner) || !ok(repo) {
        return None;
    }
    // a folder inside the download only: no `..`, no full path (`/etc`, `C:\`)
    let folder = parts.next().unwrap_or("").to_string();
    if folder.starts_with('/')
        || folder.contains(['\\', ':'])
        || folder.split('/').any(|p| p == "..")
    {
        return None;
    }
    Some((
        format!(
            "https://github.com/{owner}/{}",
            repo.trim_end_matches(".git")
        ),
        folder,
    ))
}

/// Downloads a plugin into a new folder next to where installed plugins live; answers the
/// folder that has its lightsout-plugin.toml (and the download folder, to clean up).
fn download(source: &str, r#ref: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    let (url, folder) = github(source).ok_or("write it as OWNER/REPO or OWNER/REPO/FOLDER")?;
    let tmp = config_dir()
        .join("plugins")
        .join(format!(".download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(tmp.parent().unwrap_or(Path::new(".")));
    let mut c = std::process::Command::new("git");
    c.args(["clone", "--quiet", "--depth", "1"]);
    if let Some(r) = r#ref {
        c.args(["--branch", r]);
    }
    let ok = c
        .arg(&url)
        .arg(&tmp)
        .status()
        .map_err(|_| "git isn't installed (it downloads plugins)".to_string())?
        .success();
    if !ok {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("could not download {url}"));
    }
    let _ = std::fs::remove_dir_all(tmp.join(".git"));
    let root = if folder.is_empty() {
        tmp.clone()
    } else {
        tmp.join(&folder)
    };
    Ok((root, tmp))
}

/// Moves a downloaded plugin into plugins/ID (replacing what was there).
fn put_in_place(root: &Path, tmp: &Path, id: &str) -> Result<(), String> {
    let dest = installed_dir(id);
    let _ = std::fs::remove_dir_all(&dest);
    let moved = std::fs::rename(root, &dest);
    let _ = std::fs::remove_dir_all(tmp);
    moved.map_err(|e| format!("could not put it in {}: {e}", dest.display()))
}

fn install(source: &str, r#ref: Option<String>, yes: bool) -> i32 {
    let (root, tmp) = match download(source, r#ref.as_deref()) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("lightsout: {e}");
            return 1;
        }
    };
    let m = match read_manifest(&root) {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            eprintln!("lightsout: {e}");
            return 1;
        }
    };
    let mut reg = registry();
    if reg.iter().any(|e| e.id == m.id && e.path.is_some()) {
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!(
            "lightsout: {} is linked to a folder you work on; `lightsout plugin remove {}` first",
            m.id, m.id
        );
        return 1;
    }
    if refuse_key_clashes(&m) {
        let _ = std::fs::remove_dir_all(&tmp);
        eprintln!("lightsout: {} not installed.", m.name);
        return 1;
    }
    print!("{}", describe(&m, &installed_dir(&m.id)));
    if !runs_here(&m.platforms) {
        println!(
            "  (it doesn't run on this system: {})",
            m.platforms.join(", ")
        );
    }
    if !yes && !ask("Install it?") {
        let _ = std::fs::remove_dir_all(&tmp);
        println!("Not installed.");
        return 1;
    }
    if let Err(e) = put_in_place(&root, &tmp, &m.id) {
        eprintln!("lightsout: {e}");
        return 1;
    }
    reg.retain(|e| e.id != m.id);
    reg.push(Entry {
        id: m.id.clone(),
        source: Some(source.to_string()),
        r#ref,
        path: None,
        enabled: true,
    });
    if let Err(e) = save_registry(&reg) {
        eprintln!("lightsout: could not save plugins.json: {e}");
        return 1;
    }
    println!("Installed {}.", m.name);
    0
}

fn link(dir: &str) -> i32 {
    let root = std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir));
    let m = match read_manifest(&root) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lightsout: {e}");
            return 1;
        }
    };
    let mut reg = registry();
    if reg.iter().any(|e| e.id == m.id && e.source.is_some()) {
        eprintln!(
            "lightsout: {} is installed; `lightsout plugin remove {}` first",
            m.id, m.id
        );
        return 1;
    }
    if refuse_key_clashes(&m) {
        eprintln!("lightsout: {} not linked.", m.name);
        return 1;
    }
    reg.retain(|e| e.id != m.id);
    let shown = root
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    reg.push(Entry {
        id: m.id.clone(),
        source: None,
        r#ref: None,
        path: Some(shown.clone()),
        enabled: true,
    });
    if let Err(e) = save_registry(&reg) {
        eprintln!("lightsout: could not save plugins.json: {e}");
        return 1;
    }
    println!(
        "Linked {} ({shown}). Changes to its files apply at once.",
        m.name
    );
    0
}

fn update(only: Option<&str>, yes: bool) -> i32 {
    let mut code = 0;
    let reg = registry();
    let chosen: Vec<&Entry> = reg
        .iter()
        .filter(|e| e.source.is_some() && only.is_none_or(|id| e.id == id))
        .collect();
    if chosen.is_empty() {
        println!("Nothing to update (linked plugins are the folder you work on).");
        return if only.is_some() { 1 } else { 0 };
    }
    for e in chosen {
        let source = e.source.clone().unwrap_or_default();
        let (root, tmp) = match download(&source, e.r#ref.as_deref()) {
            Ok(x) => x,
            Err(err) => {
                eprintln!("lightsout: {}: {err}", e.id);
                code = 1;
                continue;
            }
        };
        let new = match read_manifest(&root) {
            Ok(m) if m.id == e.id => m,
            Ok(m) => {
                let _ = std::fs::remove_dir_all(&tmp);
                eprintln!(
                    "lightsout: {source} is now plugin `{}`, not `{}`",
                    m.id, e.id
                );
                code = 1;
                continue;
            }
            Err(err) => {
                let _ = std::fs::remove_dir_all(&tmp);
                eprintln!("lightsout: {}: {err}", e.id);
                code = 1;
                continue;
            }
        };
        if refuse_key_clashes(&new) {
            let _ = std::fs::remove_dir_all(&tmp);
            eprintln!("lightsout: {} not updated (your version stays).", e.id);
            code = 1;
            continue;
        }
        // a program it didn't start before: asked again
        let before = read_manifest(&installed_dir(&e.id))
            .map(|m| programs(&m))
            .unwrap_or_default();
        let added: Vec<String> = programs(&new)
            .into_iter()
            .filter(|p| !before.contains(p))
            .collect();
        if !added.is_empty() {
            print!("{}", describe(&new, &installed_dir(&e.id)));
            println!("  New since your version: {}", added.join("; "));
            if !yes && !ask("Update it?") {
                let _ = std::fs::remove_dir_all(&tmp);
                println!("{} not updated.", e.id);
                continue;
            }
        }
        match put_in_place(&root, &tmp, &e.id) {
            Ok(()) => println!("{} {}", new.name, new.version),
            Err(err) => {
                eprintln!("lightsout: {err}");
                code = 1;
            }
        }
    }
    code
}

fn remove(id: &str) -> i32 {
    let mut reg = registry();
    let Some(e) = reg.iter().find(|e| e.id == id).cloned() else {
        eprintln!("lightsout: no plugin `{id}` (lightsout plugin list)");
        return 1;
    };
    if e.path.is_none() {
        let _ = std::fs::remove_dir_all(installed_dir(id));
    }
    reg.retain(|x| x.id != id);
    if let Err(err) = save_registry(&reg) {
        eprintln!("lightsout: could not save plugins.json: {err}");
        return 1;
    }
    println!(
        "Removed {id}.{}",
        if settings_dir(id).exists() {
            format!(" Its settings stay in {}.", settings_dir(id).display())
        } else {
            String::new()
        }
    );
    0
}

fn switch(id: &str, on: bool) -> i32 {
    let mut reg = registry();
    let Some(e) = reg.iter_mut().find(|e| e.id == id) else {
        eprintln!("lightsout: no plugin `{id}` (lightsout plugin list)");
        return 1;
    };
    e.enabled = on;
    if let Err(err) = save_registry(&reg) {
        eprintln!("lightsout: could not save plugins.json: {err}");
        return 1;
    }
    println!("{id} is {}.", if on { "on" } else { "off" });
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plugin_and_refuses_what_would_break() {
        let dir = std::env::temp_dir().join(format!("lo-plugin-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let write = |s: &str| std::fs::write(dir.join("lightsout-plugin.toml"), s).unwrap();
        write(
            r#"
id = "battery-line"
name = "Battery line"
version = "0.1.0"
min_lightsout = "0.1"
[[commands]]
id = "show"
title = "Show battery details"
run = ["./battery", "--details"]
[[keys]]
key = "ctrl+alt+b"
command = "battery-line.show"
[[events]]
on = "agent.needs-you"
run = ["./notify"]
"#,
        );
        let m = read_manifest(&dir).unwrap();
        assert_eq!(m.commands[0].run, ["./battery", "--details"]);
        assert!(m.commands[0].menu);
        assert_eq!(m.events[0].on, "agent.needs-you");
        write("id = \"tab\"\nname = \"x\"\nversion = \"1\"");
        assert!(read_manifest(&dir).unwrap_err().contains("Lights Out uses"));
        write(
            "id = \"x\"\nname = \"x\"\nversion = \"1\"\n[[events]]\non = \"nope\"\nrun = [\"a\"]",
        );
        assert!(read_manifest(&dir).unwrap_err().contains("doesn't exist"));
        write("id = \"x\"\nname = \"x\"\nversion = \"1\"\nmin_lightsout = \"99.0\"");
        assert!(read_manifest(&dir).unwrap_err().contains("or newer"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn github_sources_are_only_owner_repo_and_a_folder() {
        assert_eq!(
            github("ArynBuilds/lightsout-plugins/battery"),
            Some((
                "https://github.com/ArynBuilds/lightsout-plugins".into(),
                "battery".into()
            ))
        );
        assert_eq!(
            github("https://github.com/a/b.git").map(|x| x.0),
            Some("https://github.com/a/b".into())
        );
        assert_eq!(github("a"), None);
        assert_eq!(github("a/b/../../etc"), None);
        assert_eq!(github("a/b//etc"), None);
        assert_eq!(github(r"a/b/C:\Windows"), None);
        assert_eq!(github("a;rm/b"), None);
    }

    #[test]
    fn link_patterns_match_with_stars() {
        let pr = "https://github.com/*/pull/*";
        assert!(matches(pr, "https://github.com/a/b/pull/12"));
        assert!(!matches(pr, "https://github.com/a/b/issues/12"));
        assert!(!matches(pr, "http://github.com/a/b/pull/12"));
        assert!(matches("*", "anything"));
        assert!(matches("https://x.com/*", "https://x.com/"));
        assert!(matches("exact", "exact"));
        assert!(!matches("a*b*c", "a-c-b"));
    }

    #[test]
    fn versions_compare_number_by_number() {
        assert!(at_least("0.4.0-rc.1", "0.4.0"));
        assert!(at_least("0.10.0", "0.9.5"));
        assert!(!at_least("0.3.3", "0.4"));
    }
}
