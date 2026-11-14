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
    /// A command line in a popup ("popup"), a new tab ("tab") or a split ("split").
    Run {
        mode: &'static str,
        command: String,
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

impl Action {
    /// `tab.new`, `room.go 2`, `run.tab npm test`... None for a name Lights Out doesn't know
    /// or a missing argument.
    pub fn parse(command: &str) -> Option<Action> {
        let command = command.trim();
        let (id, arg) = command
            .split_once(char::is_whitespace)
            .map(|(i, a)| (i, a.trim()))
            .unwrap_or((command, ""));
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
            Action::Run { mode, command } => format!("run.{mode} {command}"),
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

/// The keys in use: your own (from the settings) first, then the defaults you didn't take
/// over. Yours may name a command (`"ctrl+alt+r" = "room.rename"`) or be empty to free a key.
pub fn keymap(own: &[(String, String)]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    for (k, cmd) in own {
        let k = key_name(k);
        if !v.iter().any(|(x, _)| *x == k) {
            v.push((k, cmd.trim().to_string()));
        }
    }
    for (k, cmd) in DEFAULT_KEYS {
        if !v.iter().any(|(x, _)| x == k) {
            v.push((k.to_string(), cmd.to_string()));
        }
    }
    v.retain(|(_, cmd)| !cmd.is_empty());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(c: &Command) -> String {
        match c.arg {
            "" => c.id.to_string(),
            "NUMBER" => format!("{} 2", c.id),
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
    fn your_keys_come_first_and_can_free_a_default() {
        let own = vec![
            ("Ctrl+Shift+T".to_string(), "room.new".to_string()),
            ("ctrl+shift+w".to_string(), String::new()),
            ("ctrl+alt+g".to_string(), "run.popup lazygit".to_string()),
        ];
        let m = keymap(&own);
        let get = |k: &str| m.iter().find(|(x, _)| x == k).map(|(_, c)| c.as_str());
        assert_eq!(get("ctrl+shift+t"), Some("room.new"));
        assert_eq!(get("ctrl+shift+w"), None);
        assert_eq!(get("ctrl+alt+g"), Some("run.popup lazygit"));
        assert_eq!(get("ctrl+shift+n"), Some("room.new"));
    }
}
