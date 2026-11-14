//! Everything Lights Out can do, as one list of named commands. The actions menu, the keys,
//! the right-click menu, the buttons, `lightsout do` and the API all run these; nothing has
//! a second path. A command is its name plus, for some, one argument: `room.go 2`,
//! `run.tab npm test`.

/// One thing Lights Out can do, as the app runs it.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Palette,
    Agents,
    NewTab,
    AskCommand,
    SplitRight,
    SplitDown,
    Focus(i32, i32),
    ClosePane,
    /// A pane floats (the focused one, or this one).
    FloatPane(Option<u64>),
    /// A floating pane goes back into the layout.
    TilePane(Option<u64>),
    /// A pane floats at this place (% of the area, from the top left).
    MovePane {
        x: u16,
        y: u16,
        pane: Option<u64>,
    },
    /// A pane floats at this size (% of the area).
    ResizePane {
        w: u16,
        h: u16,
        pane: Option<u64>,
    },
    MinimizePane(Option<u64>),
    /// A pane comes to the front (and back from minimised).
    RaisePane(Option<u64>),
    Jump,
    NextTab,
    PrevTab,
    /// The tab at this place (0 = the first).
    GoTab(usize),
    Editor,
    Copy,
    Paste,
    ScrollUp,
    ScrollDown,
    FontBigger,
    FontSmaller,
    NewRoom,
    RenameRoom,
    CloseRoom,
    /// Asks for an ssh address: a room that lives on that server.
    NewServerRoom,
    /// The room at this place (0 = the first).
    GoRoom(usize),
    ToggleSidebar,
    Settings,
    Wifi,
    /// The network at this place in the last scan.
    WifiJoin(usize),
    WifiOther,
    ScreenOff,
    /// Lights Out, or Lights On while in Lights Out (asks first).
    Lights,
    /// Close this terminal's view; the agents keep running in the background.
    Detach,
    Quit,
    /// Plugins off, or on again: the way back when a plugin broke the screen.
    Plain,
    /// A command line in a popup ("popup"), a new tab ("tab") or a split ("split").
    Run {
        mode: &'static str,
        command: String,
    },
    /// A plugin's command (`PLUGIN.COMMAND [ARG]`), with what was written after its name.
    Plugin {
        plugin: String,
        command: String,
        arg: String,
    },
}

/// A command as it is listed: its name, what it needs after the name, what it does, and
/// whether the actions menu shows it.
pub struct Command {
    pub id: &'static str,
    pub arg: &'static str,
    pub title: &'static str,
    pub menu: bool,
    /// Shown in the menu instead of the command's key (when one line stands for several).
    pub hint: &'static str,
}

const fn c(id: &'static str, title: &'static str) -> Command {
    Command {
        id,
        arg: "",
        title,
        menu: true,
        hint: "",
    }
}

const fn hidden(id: &'static str, arg: &'static str, title: &'static str) -> Command {
    Command {
        id,
        arg,
        title,
        menu: false,
        hint: "",
    }
}

/// Every command, in the order of the actions menu.
pub const COMMANDS: &[Command] = &[
    hidden("actions.open", "", "Everything Lights Out can do"),
    c("tab.new", "New tab"),
    c("agent.start", "Start an agent..."),
    hidden("command.ask", "", "Run a command in a new tab..."),
    c("pane.split-right", "Split right"),
    c("pane.split-down", "Split down"),
    Command {
        hint: "Alt+arrows",
        ..c("pane.focus-right", "Move between panes")
    },
    hidden("pane.focus-left", "", "Move to the pane on the left"),
    hidden("pane.focus-up", "", "Move to the pane above"),
    hidden("pane.focus-down", "", "Move to the pane below"),
    c("pane.close", "Close pane"),
    Command {
        arg: "[PANE]",
        ..c("pane.float", "Float this pane (move it freely)")
    },
    Command {
        arg: "[PANE]",
        ..c("pane.tile", "Put this pane back in the layout")
    },
    Command {
        arg: "[PANE]",
        ..c("pane.minimize", "Minimize this pane")
    },
    hidden(
        "pane.move",
        "X Y [PANE]",
        "Float a pane at X, Y (% of the area)",
    ),
    hidden(
        "pane.resize",
        "W H [PANE]",
        "Float a pane at this size (% of the area)",
    ),
    hidden(
        "pane.raise",
        "[PANE]",
        "Bring a pane to the front (and back from minimized)",
    ),
    c("agent.jump", "Jump to the agent that needs you"),
    c("tab.next", "Next tab"),
    c("tab.previous", "Previous tab"),
    hidden("tab.go", "NUMBER", "Go to a tab (1 = the first)"),
    c("pane.output-in-editor", "Open output in an editor"),
    c("edit.copy", "Copy selection"),
    c("edit.paste", "Paste"),
    hidden("pane.scroll-up", "", "Scroll up half a screen"),
    hidden("pane.scroll-down", "", "Scroll down half a screen"),
    c("font.bigger", "Bigger text"),
    c("font.smaller", "Smaller text"),
    c("room.new", "New room"),
    c("room.rename", "Rename this room"),
    c("room.close", "Delete this room (stops its agents)"),
    c("room.new-on-server", "New room on a server..."),
    hidden("room.go", "NUMBER", "Go to a room (1 = the first)"),
    c("sidebar.toggle", "Show / hide the sidebar"),
    c("settings.open", "Settings"),
    c("wifi.choose", "Wi-Fi..."),
    hidden(
        "wifi.join",
        "NUMBER",
        "Join a network from the last Wi-Fi list",
    ),
    hidden("wifi.other", "", "Join a network by its name..."),
    c("screen.off", "Screen off (any key turns it on)"),
    c("lights.switch", "Lights Out"),
    c(
        "app.detach",
        "Detach (close this view, agents keep running)",
    ),
    c("app.quit", "Quit (stops all agents)"),
    c(
        "app.plain",
        "Plugins off / on (the way back if one breaks the screen)",
    ),
    hidden("run.popup", "COMMAND", "Run a command line in a popup"),
    hidden("run.tab", "COMMAND", "Run a command line in a new tab"),
    hidden("run.split", "COMMAND", "Run a command line in a split"),
];

/// The keys every copy of Lights Out starts with (your settings can change or remove each).
pub const DEFAULT_KEYS: &[(&str, &str)] = &[
    ("ctrl+shift+p", "actions.open"),
    ("ctrl+shift+a", "agent.start"),
    ("ctrl+shift+t", "tab.new"),
    ("ctrl+shift+d", "app.detach"),
    ("ctrl+shift+b", "sidebar.toggle"),
    ("ctrl+shift+n", "room.new"),
    ("ctrl+shift+w", "pane.close"),
    ("ctrl+shift+j", "agent.jump"),
    ("ctrl+shift+e", "pane.output-in-editor"),
    ("ctrl+shift+c", "edit.copy"),
    ("ctrl+shift+v", "edit.paste"),
    ("ctrl+shift+l", "lights.switch"),
    ("ctrl+shift+o", "screen.off"),
    ("ctrl+shift+q", "app.quit"),
    ("ctrl+shift+f12", "app.plain"),
    ("ctrl+shift+arrowright", "pane.split-right"),
    ("ctrl+shift+arrowdown", "pane.split-down"),
    ("ctrl+shift+tab", "tab.previous"),
    ("ctrl+tab", "tab.next"),
    ("ctrl+=", "font.bigger"),
    ("ctrl++", "font.bigger"),
    ("ctrl+-", "font.smaller"),
    ("ctrl+,", "settings.open"),
    ("alt+arrowleft", "pane.focus-left"),
    ("alt+arrowright", "pane.focus-right"),
    ("alt+arrowup", "pane.focus-up"),
    ("alt+arrowdown", "pane.focus-down"),
    ("alt+1", "tab.go 1"),
    ("alt+2", "tab.go 2"),
    ("alt+3", "tab.go 3"),
    ("alt+4", "tab.go 4"),
    ("alt+5", "tab.go 5"),
    ("alt+6", "tab.go 6"),
    ("alt+7", "tab.go 7"),
    ("alt+8", "tab.go 8"),
    ("alt+9", "tab.go 9"),
    ("shift+pageup", "pane.scroll-up"),
    ("shift+pagedown", "pane.scroll-down"),
    ("shift+insert", "edit.paste"),
];

/// A place typed by people (1 = the first) as the app counts it (0 = the first).
fn place(arg: &str) -> Option<usize> {
    arg.parse::<usize>().ok().filter(|n| *n > 0).map(|n| n - 1)
}

/// An optional pane number at this place: none written (the focused pane), or a number;
/// anything else isn't understood.
fn pane(words: &[&str], i: usize) -> Option<Option<u64>> {
    match words.get(i) {
        None => Some(None),
        Some(w) => w.parse().ok().map(Some),
    }
}

/// A share of the area in %, from `min` to 100.
fn percent(words: &[&str], i: usize, min: u16) -> Option<u16> {
    words
        .get(i)?
        .parse()
        .ok()
        .filter(|n| (min..=100).contains(n))
}

/// A command's name with the pane it names, when it names one.
fn with_pane(name: &str, pane: &Option<u64>) -> String {
    match pane {
        Some(p) => format!("{name} {p}"),
        None => name.to_string(),
    }
}

impl Action {
    /// `tab.new`, `room.go 2`, `run.tab npm test`... None for a name Lights Out doesn't know
    /// or a missing argument.
    pub fn parse(command: &str) -> Option<Action> {
        let command = command.trim();
        let (id, arg) = command
            .split_once(char::is_whitespace)
            .map(|(i, a)| (i, a.trim()))
            .unwrap_or((command, ""));
        let nums: Vec<&str> = arg.split_whitespace().collect();
        let run = |mode: &'static str| {
            (!arg.is_empty()).then(|| Action::Run {
                mode,
                command: arg.to_string(),
            })
        };
        Some(match id {
            "actions.open" => Action::Palette,
            "agent.start" => Action::Agents,
            "tab.new" => Action::NewTab,
            "command.ask" => Action::AskCommand,
            "pane.split-right" => Action::SplitRight,
            "pane.split-down" => Action::SplitDown,
            "pane.focus-left" => Action::Focus(-1, 0),
            "pane.focus-right" => Action::Focus(1, 0),
            "pane.focus-up" => Action::Focus(0, -1),
            "pane.focus-down" => Action::Focus(0, 1),
            "pane.close" => Action::ClosePane,
            "pane.float" => Action::FloatPane(pane(&nums, 0)?),
            "pane.tile" => Action::TilePane(pane(&nums, 0)?),
            "pane.minimize" => Action::MinimizePane(pane(&nums, 0)?),
            "pane.raise" => Action::RaisePane(pane(&nums, 0)?),
            "pane.move" => Action::MovePane {
                x: percent(&nums, 0, 0)?,
                y: percent(&nums, 1, 0)?,
                pane: pane(&nums, 2)?,
            },
            "pane.resize" => Action::ResizePane {
                w: percent(&nums, 0, 5)?,
                h: percent(&nums, 1, 5)?,
                pane: pane(&nums, 2)?,
            },
            "agent.jump" => Action::Jump,
            "tab.next" => Action::NextTab,
            "tab.previous" => Action::PrevTab,
            "tab.go" => Action::GoTab(place(arg)?),
            "pane.output-in-editor" => Action::Editor,
            "edit.copy" => Action::Copy,
            "edit.paste" => Action::Paste,
            "pane.scroll-up" => Action::ScrollUp,
            "pane.scroll-down" => Action::ScrollDown,
            "font.bigger" => Action::FontBigger,
            "font.smaller" => Action::FontSmaller,
            "room.new" => Action::NewRoom,
            "room.rename" => Action::RenameRoom,
            "room.close" => Action::CloseRoom,
            "room.new-on-server" => Action::NewServerRoom,
            "room.go" => Action::GoRoom(place(arg)?),
            "sidebar.toggle" => Action::ToggleSidebar,
            "settings.open" => Action::Settings,
            "wifi.choose" => Action::Wifi,
            "wifi.join" => Action::WifiJoin(place(arg)?),
            "wifi.other" => Action::WifiOther,
            "screen.off" => Action::ScreenOff,
            "lights.switch" => Action::Lights,
            "app.detach" => Action::Detach,
            "app.quit" => Action::Quit,
            "app.plain" => Action::Plain,
            "run.popup" => run("popup")?,
            "run.tab" => run("tab")?,
            "run.split" => run("split")?,
            _ => return None,
        })
    }

    /// The command's name with its argument, as `parse` reads it.
    pub fn id(&self) -> String {
        let s = |s: &str| s.to_string();
        match self {
            Action::Palette => s("actions.open"),
            Action::Agents => s("agent.start"),
            Action::NewTab => s("tab.new"),
            Action::AskCommand => s("command.ask"),
            Action::SplitRight => s("pane.split-right"),
            Action::SplitDown => s("pane.split-down"),
            Action::Focus(-1, _) => s("pane.focus-left"),
            Action::Focus(1, _) => s("pane.focus-right"),
            Action::Focus(_, -1) => s("pane.focus-up"),
            Action::Focus(..) => s("pane.focus-down"),
            Action::ClosePane => s("pane.close"),
            Action::FloatPane(p) => with_pane("pane.float", p),
            Action::TilePane(p) => with_pane("pane.tile", p),
            Action::MinimizePane(p) => with_pane("pane.minimize", p),
            Action::RaisePane(p) => with_pane("pane.raise", p),
            Action::MovePane { x, y, pane } => with_pane(&format!("pane.move {x} {y}"), pane),
            Action::ResizePane { w, h, pane } => with_pane(&format!("pane.resize {w} {h}"), pane),
            Action::Jump => s("agent.jump"),
            Action::NextTab => s("tab.next"),
            Action::PrevTab => s("tab.previous"),
            Action::GoTab(i) => format!("tab.go {}", i + 1),
            Action::Editor => s("pane.output-in-editor"),
            Action::Copy => s("edit.copy"),
            Action::Paste => s("edit.paste"),
            Action::ScrollUp => s("pane.scroll-up"),
            Action::ScrollDown => s("pane.scroll-down"),
            Action::FontBigger => s("font.bigger"),
            Action::FontSmaller => s("font.smaller"),
            Action::NewRoom => s("room.new"),
            Action::RenameRoom => s("room.rename"),
            Action::CloseRoom => s("room.close"),
            Action::NewServerRoom => s("room.new-on-server"),
            Action::GoRoom(i) => format!("room.go {}", i + 1),
            Action::ToggleSidebar => s("sidebar.toggle"),
            Action::Settings => s("settings.open"),
            Action::Wifi => s("wifi.choose"),
            Action::WifiJoin(i) => format!("wifi.join {}", i + 1),
            Action::WifiOther => s("wifi.other"),
            Action::ScreenOff => s("screen.off"),
            Action::Lights => s("lights.switch"),
            Action::Detach => s("app.detach"),
            Action::Quit => s("app.quit"),
            Action::Plain => s("app.plain"),
            Action::Run { mode, command } => format!("run.{mode} {command}"),
            Action::Plugin {
                plugin,
                command,
                arg,
            } if arg.is_empty() => format!("{plugin}.{command}"),
            Action::Plugin {
                plugin,
                command,
                arg,
            } => format!("{plugin}.{command} {arg}"),
        }
    }
}

/// A key as Lights Out names it: modifiers in the order ctrl, alt, shift, then the key, all
/// lowercase (`ctrl+shift+arrowright`, `alt+1`, `ctrl++`). People may also write `Right`,
/// `Esc`, `PgUp`... and the modifiers in any order.
pub fn key_name(key: &str) -> String {
    let key = key.trim().to_lowercase();
    // the key itself may be "+" (`ctrl++`)
    let (mods, base) = match key.strip_suffix("++") {
        Some(m) => (m, "+".to_string()),
        None => match key.rsplit_once('+') {
            Some((m, b)) => (m, b.to_string()),
            None => ("", key.clone()),
        },
    };
    let has = |m: &str| mods.split('+').any(|x| x == m);
    let base = match base.as_str() {
        "left" | "right" | "up" | "down" => format!("arrow{base}"),
        "esc" => "escape".into(),
        "del" => "delete".into(),
        "ins" => "insert".into(),
        "pgup" => "pageup".into(),
        "pgdn" | "pgdown" => "pagedown".into(),
        "return" => "enter".into(),
        _ => base,
    };
    let mut parts: Vec<&str> = Vec::new();
    if has("ctrl") || has("control") {
        parts.push("ctrl");
    }
    if has("alt") || has("option") {
        parts.push("alt");
    }
    if has("shift") {
        parts.push("shift");
    }
    parts.push(&base);
    parts.join("+")
}

/// A key name as people read it: `ctrl+shift+arrowright` → `Ctrl+Shift+Right`.
pub fn key_label(name: &str) -> String {
    let (mods, base) = match name.strip_suffix("++") {
        Some(m) => (m, "+"),
        None => name.rsplit_once('+').unwrap_or(("", name)),
    };
    let word = |w: &str| -> String {
        match w {
            "ctrl" => "Ctrl".into(),
            "pageup" => "PageUp".into(),
            "pagedown" => "PageDown".into(),
            w if w.starts_with("arrow") && w.len() > 5 => word_case(&w[5..]),
            w => word_case(w),
        }
    };
    mods.split('+')
        .filter(|m| !m.is_empty())
        .map(word)
        .chain(std::iter::once(word(base)))
        .collect::<Vec<_>>()
        .join("+")
}

fn word_case(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Plugins off and on again (`app.plain`): checked before every other key, and nobody can
/// take it, so a plugin that broke the screen can always be switched off.
pub const SAFE_KEY: &str = "ctrl+shift+f12";

/// Who a key belongs to.
#[derive(Clone, Debug, PartialEq)]
pub enum Owner {
    You,
    Plugin(String),
    LightsOut,
}

impl Owner {
    pub fn name(&self) -> String {
        match self {
            Owner::You => "your settings".into(),
            Owner::Plugin(id) => id.clone(),
            Owner::LightsOut => "Lights Out".into(),
        }
    }
}

/// A key in use: the command it runs and whose it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub key: String,
    pub command: String,
    pub owner: Owner,
}

/// A plugin's key that someone had first, so it does nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Skipped {
    pub plugin: String,
    pub key: String,
    pub command: String,
    pub taken: Bound,
}

/// The keys in use. Yours (from the settings) first; then each plugin's, in the order they
/// were added (a key someone has already is skipped); then Lights Out's own, where nobody
/// took the key. Yours may name a command (`"ctrl+alt+r" = "room.rename"`), or be empty to
/// take a key away from Lights Out (a plugin may then have it). `plugins`: (plugin, key,
/// command).
pub fn bindings(
    own: &[(String, String)],
    plugins: &[(String, String, String)],
) -> (Vec<Bound>, Vec<Skipped>) {
    // the way back when a plugin broke the screen: always Lights Out's, first
    let mut v: Vec<Bound> = vec![Bound {
        key: SAFE_KEY.into(),
        command: "app.plain".into(),
        owner: Owner::LightsOut,
    }];
    let mut freed: Vec<String> = Vec::new();
    for (k, cmd) in own {
        let (key, command) = (key_name(k), cmd.trim().to_string());
        if key == SAFE_KEY {
            continue;
        }
        if command.is_empty() {
            freed.push(key);
        } else if !v.iter().any(|b| b.key == key) {
            v.push(Bound {
                key,
                command,
                owner: Owner::You,
            });
        }
    }
    let mut skipped = Vec::new();
    for (plugin, k, cmd) in plugins {
        let key = key_name(k);
        match v.iter().find(|b| b.key == key) {
            // the same command already on it (you gave it the same key): nothing lost
            Some(b) if b.command == cmd.trim() => {}
            Some(b) => skipped.push(Skipped {
                plugin: plugin.clone(),
                key,
                command: cmd.trim().to_string(),
                taken: b.clone(),
            }),
            None => v.push(Bound {
                key,
                command: cmd.trim().to_string(),
                owner: Owner::Plugin(plugin.clone()),
            }),
        }
    }
    for (k, cmd) in DEFAULT_KEYS {
        if !freed.iter().any(|f| f == k) && !v.iter().any(|b| b.key == *k) {
            v.push(Bound {
                key: k.to_string(),
                command: cmd.to_string(),
                owner: Owner::LightsOut,
            });
        }
    }
    (v, skipped)
}

/// The keys in use, as (key, command).
pub fn keymap(
    own: &[(String, String)],
    plugins: &[(String, String, String)],
) -> Vec<(String, String)> {
    bindings(own, plugins)
        .0
        .into_iter()
        .map(|b| (b.key, b.command))
        .collect()
}

/// A new plugin's keys that are already used (by you, another plugin, or Lights Out where
/// you didn't free the key), and twice in itself: each as a sentence saying who has it and
/// how to fix it. Empty: it may have them all.
pub fn clashes(
    own: &[(String, String)],
    others: &[(String, String, String)],
    plugin: &str,
    keys: &[(String, String)],
) -> Vec<String> {
    let (bound, _) = bindings(own, others);
    let mut out = Vec::new();
    for (i, (k, _)) in keys.iter().enumerate() {
        let key = key_name(k);
        if keys[..i].iter().any(|(x, _)| key_name(x) == key) {
            out.push(format!("{key} is in {plugin} twice"));
            continue;
        }
        if let Some(b) = bound.iter().find(|b| b.key == key) {
            if key == SAFE_KEY {
                out.push(format!(
                    "{key} is Lights Out's way back when a plugin breaks the screen (app.plain): nobody can take it. Change the key in {plugin}."
                ));
                continue;
            }
            let fix = match &b.owner {
                Owner::You => "or change it in your settings".to_string(),
                Owner::Plugin(id) => format!("or switch {id} off (lightsout plugin off {id})"),
                Owner::LightsOut => {
                    format!("or free it in your settings: \"{key}\" = \"\"")
                }
            };
            out.push(format!(
                "{key} is already used by {} ({}). Change the key in {plugin}, {fix}.",
                b.owner.name(),
                b.command
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(c: &Command) -> String {
        match c.arg {
            "" => c.id.to_string(),
            "NUMBER" => format!("{} 2", c.id),
            "[PANE]" => format!("{} 7", c.id),
            "X Y [PANE]" | "W H [PANE]" => format!("{} 30 40 7", c.id),
            _ => format!("{} npm test", c.id),
        }
    }

    #[test]
    fn every_listed_command_runs_and_keeps_its_name() {
        for c in COMMANDS {
            let s = sample(c);
            let a = Action::parse(&s).unwrap_or_else(|| panic!("{s} isn't known"));
            assert_eq!(a.id(), s);
        }
        assert_eq!(COMMANDS.iter().filter(|c| c.id == "tab.new").count(), 1);
    }

    #[test]
    fn every_default_key_runs_a_listed_command() {
        for (k, cmd) in DEFAULT_KEYS {
            assert_eq!(
                key_name(k),
                *k,
                "{k} isn't written as Lights Out names keys"
            );
            let a = Action::parse(cmd).unwrap_or_else(|| panic!("{cmd} isn't known"));
            let id = a.id();
            let id = id.split(' ').next().unwrap();
            assert!(COMMANDS.iter().any(|c| c.id == id), "{id} isn't listed");
        }
    }

    #[test]
    fn unknown_names_and_missing_arguments_run_nothing() {
        assert_eq!(Action::parse("tab.nope"), None);
        assert_eq!(Action::parse("room.go"), None);
        assert_eq!(Action::parse("room.go 0"), None);
        assert_eq!(Action::parse("run.tab"), None);
        assert_eq!(Action::parse("  room.go   3 "), Some(Action::GoRoom(2)));
        assert_eq!(
            Action::parse("run.split npm run dev -- --port 3000"),
            Some(Action::Run {
                mode: "split",
                command: "npm run dev -- --port 3000".into()
            })
        );
    }

    #[test]
    fn keys_are_named_one_way_however_they_are_written() {
        assert_eq!(key_name("Shift+Ctrl+Right"), "ctrl+shift+arrowright");
        assert_eq!(key_name("ctrl++"), "ctrl++");
        assert_eq!(key_name("Alt+PgUp"), "alt+pageup");
        assert_eq!(key_name("F5"), "f5");
        assert_eq!(key_label("ctrl+shift+arrowright"), "Ctrl+Shift+Right");
        assert_eq!(key_label("ctrl++"), "Ctrl++");
        assert_eq!(key_label("ctrl+,"), "Ctrl+,");
        assert_eq!(key_label("shift+pageup"), "Shift+PageUp");
        assert_eq!(key_label("ctrl+shift+t"), "Ctrl+Shift+T");
    }

    #[test]
    fn your_keys_come_first_then_plugins_then_the_defaults() {
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        let own = vec![
            s("Ctrl+Shift+T", "room.new"),
            s("ctrl+shift+w", ""),
            s("ctrl+alt+g", "run.popup lazygit"),
        ];
        let p = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        let plugins = vec![
            p("git-tools", "ctrl+alt+g", "git-tools.status"),
            p("notes", "ctrl+shift+j", "notes.open"),
            p("other", "ctrl+shift+j", "other.open"),
        ];
        let (bound, skipped) = bindings(&own, &plugins);
        let get = |k: &str| {
            bound
                .iter()
                .find(|b| b.key == k)
                .map(|b| b.command.as_str())
        };
        assert_eq!(get("ctrl+shift+t"), Some("room.new"));
        // freed by you: gone from Lights Out, nobody has it
        assert_eq!(get("ctrl+shift+w"), None);
        assert_eq!(get("ctrl+alt+g"), Some("run.popup lazygit"));
        // a plugin before Lights Out's own; the first plugin before the next one
        assert_eq!(get("ctrl+shift+j"), Some("notes.open"));
        assert_eq!(get("ctrl+shift+n"), Some("room.new"));
        let who: Vec<(&str, &str)> = skipped
            .iter()
            .map(|s| (s.plugin.as_str(), s.taken.command.as_str()))
            .collect();
        assert_eq!(
            who,
            [("git-tools", "run.popup lazygit"), ("other", "notes.open")]
        );
    }

    #[test]
    fn a_new_plugin_cant_take_a_key_in_use() {
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        let own = vec![s("ctrl+alt+r", "room.rename"), s("ctrl+shift+w", "")];
        let others = vec![(
            "git-tools".to_string(),
            "ctrl+alt+g".to_string(),
            "git-tools.status".to_string(),
        )];
        let keys = vec![
            s("Ctrl+Alt+G", "mine.a"),
            s("ctrl+alt+r", "mine.b"),
            s("ctrl+shift+t", "mine.c"),
            s("ctrl+shift+w", "mine.d"),
            s("ctrl+alt+x", "mine.e"),
            s("ctrl+alt+x", "mine.f"),
        ];
        let c = clashes(&own, &others, "mine", &keys);
        assert_eq!(c.len(), 4, "{c:?}");
        assert!(c[0].starts_with("ctrl+alt+g is already used by git-tools (git-tools.status)"));
        assert!(c[1].contains("by your settings (room.rename)"));
        assert!(
            c[2].contains("by Lights Out (tab.new)") && c[2].contains("\"ctrl+shift+t\" = \"\"")
        );
        assert_eq!(c[3], "ctrl+alt+x is in mine twice");
        // the way back can't be taken, not even after freeing it
        let own = vec![s("ctrl+shift+f12", "")];
        let c = clashes(&own, &[], "mine", &[s("Ctrl+Shift+F12", "mine.a")]);
        assert!(c[0].contains("nobody can take it"), "{c:?}");
        let (bound, _) = bindings(&[s("ctrl+shift+f12", "room.new")], &[]);
        let safe: Vec<_> = bound.iter().filter(|b| b.key == SAFE_KEY).collect();
        assert_eq!((safe.len(), safe[0].command.as_str()), (1, "app.plain"));
    }
}
