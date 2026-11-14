# Configuration

Lights Out works without any settings. When you want to change something, open **actions → Settings** (Ctrl+,). It opens your settings file, with every option explained. Saved changes apply right away, with no restart.

The file is `~/.config/lightsout/config.toml` (on Windows: `%USERPROFILE%\.config\lightsout\config.toml`). Windows and Lights Out mode share it.

## The settings file

```toml
[theme]
background = "#000000"   # full black: OLED pixels are switched off
foreground = "#d6d6d6"
accent = "#f5c542"
font_size = 0            # 0 = automatic

[notify]
popup = true             # a popup in the window when an agent finishes or needs you
sound = false            # a short sound with it (Windows, Mac)
sound_done = ""          # your own .wav files
sound_needs_you = ""

[status]
# extra phrases that mean "this program is waiting for my answer"
needs_you = []

[history]
keep = true              # bring rooms, tabs and conversations back after Quit or a restart

[parts]
sidebar = "left"         # left, right or off (Ctrl+Shift+B shows and hides it)
tabs = "top"             # top, bottom or off

[bar]
# extra items in the top bar, e.g.
# items = [ { text = "my laptop" }, { command = "git branch --show-current", every = 30 } ]
items = []
```

## Your own keys

A key can run any action (every name is in `lightsout commands`), a command line in a popup, a new tab or a split, or nothing:

```toml
[keys]
"ctrl+alt+r" = "room.rename"                              # any action
"ctrl+shift+t" = "room.new"                               # take over a Lights Out key
"alt+g" = "run.popup lazygit"                             # a command line, as an action
"ctrl+alt+g" = { command = "lazygit", mode = "popup" }    # the same, written out
"ctrl+alt+t" = { command = "npm test", mode = "split" }   # mode: popup, tab or split
"ctrl+shift+w" = ""                                       # free the key: your programs get it
```

Keys are written as you like (`Ctrl+Alt+R`, `alt+ctrl+r`, `ctrl+shift+right`, `f5`...). Your keys come before any [plugin's](plugins.md#keys) and Lights Out's. Command lines from your keys also show in the actions menu as **Run: ...**. One key stays Lights Out's: Ctrl+Shift+F12, which switches plugins off ([When a plugin breaks the screen](plugins.md#when-a-plugin-breaks-the-screen)).

## Colours and the look

The colours under `[theme]` are Lights Out's and every plugin's: plugins get them (`LIGHTSOUT_THEME`), and their parts can name them, so one change recolours everything. `[parts]` moves the sidebar to the right or the tab bar to the bottom, or switches either off. [Plugins](plugins.md#parts-of-the-screen) can add more parts on any side, or replace these two.

## When an agent asks in its own words

Lights Out recognises common questions by itself. If a tool waits for you in a way it misses, add the phrase:

```toml
[status]
needs_you = ["Approve this plan?", "Type the code"]
```

## Rooms on a server

A room can live on a server: its agents run there, 24/7, also while your computer sleeps or is off. It is not a setting: add it with a command, or **actions → New room on a server...**

```sh
lightsout room add me@server.example.com        # the room "server"
lightsout room add me@10.0.0.5 desktop          # or give it a name
lightsout room add build --on buildbox          # a name from ~/.ssh/config
lightsout room list
lightsout room remove desktop                   # leaves it here; it keeps running on the server
```

The server needs Lights Out too (`curl -fsSL https://lightsout.sh | sh`). Logins use your normal SSH setup (keys, `~/.ssh/config`); Lights Out keeps no passwords. To see everything on a server at once: `lightsout open me@server`.

## Data files

Two plain files describe how tabs are handed over. Your own copies in `~/.config/lightsout/` add to the built-in ones (yours win).

| File | What it holds |
|---|---|
| `resume.conf` | How each agent continues its own conversation. See [Restoring your tabs](session-state.md#adding-an-agent). |
| `per-system.conf` | Folders in your home that hold programs for one system only (for example `.cargo`, `.nvm`), so Windows and Lights Out each keep their own copy. |

## Where things are kept

| What | Where |
|---|---|
| Settings, your keys, `resume.conf` | `~/.config/lightsout/` |
| Plugins | `~/.config/lightsout/plugins/`, `plugins.json`, `plugin-settings/` ([Plugins](plugins.md#where-things-are)) |
| Saved tabs, plugins' own files | Windows: `%LOCALAPPDATA%\lightsout`; Linux and Mac: `~/.local/state/lightsout` |
| The Lights Out system (Windows) | `C:\LightsOut` |
