# Plugins

A plugin is a folder with a `lightsout-plugin.toml` file and programs in any language: shell, Node, Python, Rust, anything. It can add:

- **commands**, which join the actions menu, `lightsout commands` and `lightsout do`, and can run unseen or on screen in a popup, tab or split
- **keys** for any command
- **programs that run on events**: an agent needs you, a room is renamed, Lights Out starts...
- **parts of the screen** on any side, any number: a status line, a second sidebar, a taskbar, or one that replaces the sidebar or the tab bar
- **frames**: parts that show a picture the plugin draws (a web page, a chart, an image)
- **link handlers**: Ctrl+click on links like `https://github.com/*/pull/*` runs its command

Programs talk back to Lights Out with the same `lightsout` commands an agent uses (see [Commands](commands.md)), so a plugin can do anything Lights Out can. With [floating panes](commands.md#floating-panes) and parts, a plugin can even change the whole look: a Windows-like screen with rooms as apps is a plugin.

Find plugins at [lightsout.sh/plugins](https://lightsout.sh/plugins).

## A first plugin

```toml
# lightsout-plugin.toml
id = "hello"
name = "Hello"
version = "0.1.0"

[[commands]]
id = "greet"                      # lightsout do hello.greet
title = "Say hello"               # its line in the actions menu
run = ["sh", "./greet.sh"]        # ./ = this plugin's folder

[[keys]]
key = "ctrl+alt+h"
command = "hello.greet"

[[events]]
on = "agent.needs-you"
run = ["sh", "./ping.sh"]
```

```sh
#!/bin/sh
# greet.sh: says hello in a popup, with what was written after the command's name
lightsout notify "hello $LIGHTSOUT_ARG"
```

```sh
lightsout plugin link ./hello     # use it from where it is, while you make it
lightsout do hello.greet world    # or Ctrl+Alt+H, or actions → Say hello
```

Changes apply at once, without restarting Lights Out: edit the files, and the next time the plugin is used it is read again.

## The file

```toml
id = "my-plugin"              # lowercase letters, digits, - and _ (also its commands' first name)
name = "My plugin"
version = "1.2.0"
min_lightsout = "0.4.0"       # optional: refused on older versions
description = "What it does"  # optional, shown when installing
platforms = ["windows", "linux", "macos"]   # optional: where it runs (default: everywhere)

[[commands]]
id = "show"
title = "Show the log"
run = ["./log-view", "--follow"]
place = "popup"               # background (default), popup, tab, split, split-down
menu = true                   # in the actions menu (default)
platforms = ["linux"]         # optional, for this command only

[[keys]]
key = "ctrl+alt+l"            # written as you like: Ctrl+Alt+L, alt+ctrl+l...
command = "my-plugin.show"    # any command: the plugin's, another plugin's, a built-in one

[[events]]
on = "room.renamed"
run = ["sh", "./renamed.sh"]

[[parts]]
id = "status"                 # its name: my-plugin.status
side = "bottom"               # left, right, top, bottom
size = 1                      # columns (left, right) or rows (top, bottom)
replaces = ""                 # optional: "sidebar" or "tabs" (the built-in one goes)
show = "always"               # or "when-filled": takes room only while it shows something
run = ["sh", "./status.sh"]   # optional: started when the part appears, fills it

[[links]]
pattern = "https://github.com/*/pull/*"   # * stands for anything
command = "my-plugin.open-pr"
```

A command can come once per system under the same name (one with `platforms = ["windows"]`, one with `["linux", "macos"]`): each system uses its own.

## Commands

`run` is a list: the program, then its arguments, with no shell in between. Any word starting with `./` or `../` is in the plugin's folder.

- **Unseen (`place = "background"`, the default):** the program runs in the plugin's folder, without waiting. What it prints goes to its log (see [Where things are](#where-things-are)).
- **On screen (`popup`, `tab`, `split`, `split-down`):** it runs in a pane, in the folder of the pane you're in, like any program you type. A popup closes when the program ends.

What was written after the command's name (`lightsout do my-plugin.show today`) comes as `LIGHTSOUT_ARG`.

## Parts of the screen

A part is a strip on one side of the screen: left and right ones are as tall as the window, top and bottom ones as wide as what is left. Parts come after the sidebar (left and right) and the tab bar (top and bottom), in the order plugins were added. A part shrinks when the panes would get too little room.

Its program fills it, and keeps it current from events:

```sh
lightsout part set my-plugin.status "3 agents working"     # plain text (lines)
lightsout part set status '[["● ", {"text": "claude", "color": "accent", "command": "agent.jump"}]]'
lightsout part clear status
lightsout part list                                         # where every part is, and what it shows
```

Inside the plugin's own programs, the part's short name is enough (`status`). What a part shows can be:

- `"one line\nanother"`: plain lines
- `["a line", ["two", "pieces"]]`: rows; a row is a line or a list of pieces
- `{"bg": "#101014", "rows": [...]}`: with a background

A piece is a string or `{"text": "...", "color": "accent", "bold": true, "command": "room.go 2"}`: clicking it runs the command (any command: built-in, a plugin's, `run.popup ...`). Colours are `#rrggbb`, or names that follow the user's [colours](configuration.md#the-settings-file): `fg`, `dim`, `accent`, `alert`, `done`, `bg`. Every program also gets them as JSON in `LIGHTSOUT_THEME`, so plugins from different people match.

A part with `replaces = "sidebar"` or `"tabs"` takes the place of the built-in one, so a plugin can redo the whole frame of the screen. The user can also move the built-in sidebar and tab bar, or switch them off ([Configuration](configuration.md#the-settings-file)).

## Frames

A part can show a picture its plugin drew, at its exact size: a web page, a graph, an image.

```sh
lightsout part image my-plugin.view /path/to/picture.png   # a PNG, or a PPM (P6)
```

The plugin hears the part's size when it appears and whenever it changes (`part.resized`: `cols`, `rows`, and `width` and `height` in pixels), and draws again then. Clicks and the mouse wheel over the part come as `part.clicked` (where, in pixels and in cells, and which button) and `part.scrolled`. In the window and in Lights Out mode the picture shows as it is; inside another terminal, where pictures can't show, the part shows its name (there, sizes are in cells, so `width` equals `cols`).

A PPM is the simplest to write from any language: the line `P6`, the width and height, `255`, then the red, green and blue bytes of each pixel.

## Events

| Event | When | `LIGHTSOUT_EVENT_JSON` has |
|---|---|---|
| `app.started` | Lights Out started (tabs already back) | |
| `pane.opened` | a pane opened | `pane`, `command`, `cwd` |
| `pane.closed` | a pane closed | `pane` |
| `program.started` | a program started in a pane | `pane`, `program` |
| `program.ended` | the program in a pane ended | `pane`, `program` |
| `agent.needs-you` | an agent asks something | `pane`, `title` |
| `agent.done` | an agent finished | `pane`, `title` |
| `room.opened` | a room was made | `room` |
| `room.renamed` | a room got a new name | `room`, `before` |
| `room.closed` | a room was closed | `room` |
| `lights.out` | Lights Out is starting (tabs saved) | |
| `lights.on` | Lights On: going back (tabs saved) | |
| `screen.off` | the screen was switched off | |
| `battery.low` | the battery is low | `percent` |
| `part.resized` | a part has a new size | `part`, `cols`, `rows`, `width`, `height` |
| `part.clicked` | a part was clicked (not on a piece with a command) | `part`, `x`, `y`, `col`, `row`, `button` |
| `part.scrolled` | the mouse wheel turned over a part | `part`, `lines` (up > 0), `x`, `y` |

`lightsout plugin events` prints this list.

## Links

Ctrl+click on a web link in any pane: the first plugin whose `pattern` matches runs its `command` with the link (a plugin's command gets it as `LIGHTSOUT_ARG`; any other command gets it at the end, quoted, so `command = "run.popup gh pr view"` works). No plugin matches: the link opens in your browser.

## What a program gets

| Variable | Is |
|---|---|
| `LIGHTSOUT` | `1` |
| `LIGHTSOUT_SESSION` | the Lights Out it belongs to (the `lightsout` command uses it) |
| `LIGHTSOUT_PANE` | the pane you're in (a program on screen: its own pane) |
| `LIGHTSOUT_PLUGIN_ID`, `LIGHTSOUT_PLUGIN_ROOT` | the plugin, and its folder |
| `LIGHTSOUT_PLUGIN_CONFIG` | a folder for the user's settings of the plugin (kept when it is updated or removed) |
| `LIGHTSOUT_PLUGIN_STATE` | a folder for the plugin's own files |
| `LIGHTSOUT_PLUGIN_CONTEXT` | JSON: `session`, `room`, `tab`, `pane`, `cwd`, `program`, `selection` (the selected text) |
| `LIGHTSOUT_THEME` | JSON: the user's colours (`background`, `foreground`, `accent`, `dim`, `alert`, `done`) and `font_size` |
| `LIGHTSOUT_COMMAND`, `LIGHTSOUT_ARG` | for a command: its name, and what was written after it |
| `LIGHTSOUT_EVENT`, `LIGHTSOUT_EVENT_JSON` | for an event: its name, and what it carries |
| `LIGHTSOUT_PART`, `LIGHTSOUT_PART_COLS`, `LIGHTSOUT_PART_ROWS`, `LIGHTSOUT_PART_WIDTH`, `LIGHTSOUT_PART_HEIGHT` | for a part's program: which part, and its size |

## Keys

Your own keys come first, then plugins' keys (in the order the plugins were added), then Lights Out's.

- A plugin whose key is already used, by your settings, another plugin or Lights Out, **isn't installed or linked**. The message says who has the key and how to fix it: pick another key, or free a Lights Out key in your settings with `"ctrl+shift+t" = ""`.
- If you take a plugin's key later in your settings, yours wins, and a popup says once which plugin key now does nothing.
- **Ctrl+Shift+F12** belongs to nobody but Lights Out: see below.

## When a plugin breaks the screen

**Ctrl+Shift+F12** (inside another terminal: Ctrl+B, then F12) switches every plugin off, at once, until you press it again: their parts, keys and commands go, and the screen is Lights Out's own. No setting or plugin can take this key. To start with plugins off: `lightsout --plain`. Then `lightsout plugin off ID` or `lightsout plugin remove ID` the one that broke it.

## Our web plugin

A web page in a panel on the right, drawn by the browser you already have (Edge, Chrome or Chromium, without a window):

```sh
lightsout plugin install ArynBuilds/lightsout/plugins/web
lightsout do web.open https://example.com
```

`web.reload`, `web.browser` (open it in your browser; a click on the page does the same) and `web.close` (the panel goes, and its room returns to the panes). The page is a picture: it is drawn when you open or reload it and when the panel changes size, and nothing runs in between, so it costs no power while you read. Inside another terminal the panel tells you the page shows in the window.

## Installing, updating, removing

```sh
lightsout plugin install OWNER/REPO               # a plugin from GitHub
lightsout plugin install OWNER/REPO/FOLDER        # one in a folder of a repo
lightsout plugin install OWNER/REPO --ref v1.2.0  # a tag or branch
lightsout plugin link FOLDER                      # one you are making, used where it is
lightsout plugin list
lightsout plugin update [ID]                      # the newest version of installed ones
lightsout plugin off ID | on ID
lightsout plugin remove ID                        # its settings stay
```

Installing needs `git`. Before anything is installed, Lights Out shows every program the plugin can start and asks; `--yes` skips the question. An update that adds a program it didn't start before asks again.

## Where things are

| What | Where |
|---|---|
| Installed plugins | `~/.config/lightsout/plugins/ID/` |
| Which plugins there are, and whether each is on | `~/.config/lightsout/plugins.json` |
| Your settings for a plugin | `~/.config/lightsout/plugin-settings/ID/` |
| A plugin's own files and its log (`log.txt`) | Windows: `%LOCALAPPDATA%\lightsout\plugin-data\ID\`; Linux and Mac: `~/.local/state/lightsout/plugin-data/ID/` |

On Windows, `~` is your user folder. In Lights Out mode on a Windows PC, your plugins come along: they run there if they run on Linux.

## Power

Plugins run only when something calls them: a command, a key, an event, a part appearing. Nothing checks on a timer, so a plugin costs nothing while nothing happens, also in Lights Out mode. A plugin that starts its own long-running program is that program's responsibility: keep it light.

## Trust

A plugin runs as you, like any program you install, and can use every Lights Out command. Install plugins you trust, and read what the install shows. [lightsout.sh/plugins](https://lightsout.sh/plugins) lists our own plugins and public repos tagged `lightsout-plugin`: a list, not a review.

## Publishing

Put your plugin on GitHub (at the top of the repo, or in a folder), and tag the repo `lightsout-plugin`: it shows at [lightsout.sh/plugins](https://lightsout.sh/plugins) within an hour. Users install it with `lightsout plugin install YOU/REPO`.
