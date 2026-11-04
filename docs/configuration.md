# Configuration

Lights Out works without any settings. When you want to change something, open **actions → Settings** (Ctrl+,). It opens your settings file, with every option explained. Saved changes apply right away, with no restart.

The file is `~/.config/lightsout/config.toml` (on Windows: `%USERPROFILE%\.config\lightsout\config.toml`).

## The settings file

```toml
[theme]
background = "#000000"   # full black: OLED pixels are switched off
foreground = "#d6d6d6"
font_size = 0            # 0 = automatic

[notify]
popup = true             # a popup when an agent finishes or needs you
sound = true             # a short sound with it
sound_done = ""          # your own sound files
sound_needs_you = ""

[status]
# extra phrases that mean "this program is waiting for my answer"
needs_you = []

[history]
keep = true              # bring tabs, splits and screen history back after a restart

[bar]
# extra items in the top bar, e.g.
# items = [ { text = "my laptop" }, { command = "git branch --show-current", every = 30 } ]
items = []
```

## Your own keys

Run any command with a key: in a popup, a new tab, or a split.

```toml
[keys]
"ctrl+alt+g" = { command = "lazygit", mode = "popup" }
"ctrl+alt+t" = { command = "npm test", mode = "split" }
```

## Saved machines

Machines you open over SSH, listed in **actions**:

```toml
[[machine]]
name = "desktop"
ssh = "me@desktop.local"
```

## When an agent asks in its own words

Lights Out recognises common questions by itself. If a tool waits for you in a way it misses, add the phrase:

```toml
[status]
needs_you = ["Approve this plan?", "Type the code"]
```

## Data files

Two plain files describe how tabs are handed over. Your own copies in `~/.config/lightsout/` win over the built-in ones.

| File | What it holds |
|---|---|
| `resume.conf` | How a program continues after a switch: `program = command`. See [Agents](agents.md#how-conversations-continue). |
| `per-system.conf` | Folders in your home that hold programs for one system only (for example `.cargo`, `.nvm`), so Windows and Lights Out each keep their own copy. |

## Where things are kept

| What | Where |
|---|---|
| Settings | `~/.config/lightsout/` |
| Saved tabs | Windows: `%LOCALAPPDATA%\lightsout`; Linux and Mac: `~/.local/state/lightsout` |
| The Lights Out system (Windows) | `C:\LightsOut` |
