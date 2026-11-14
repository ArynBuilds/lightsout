# Plugins

A plugin is a folder with a `lightsout-plugin.toml` file and programs in any language: shell, Node, Python, Rust, anything. It can add:

- **commands**, which join the actions menu, `lightsout commands` and `lightsout do`, and can run unseen or on screen in a popup, tab or split
- **keys** for any command
- **programs that run on events**: an agent needs you, a room is renamed, Lights Out starts...
- **link handlers**: Ctrl+click on links like `https://github.com/*/pull/*` runs its command

Programs talk back to Lights Out with the same `lightsout` commands an agent uses (see [Commands](commands.md)), so a plugin can do anything Lights Out can.

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

[[links]]
pattern = "https://github.com/*/pull/*"   # * stands for anything
command = "my-plugin.open-pr"
```

## Commands

`run` is a list: the program, then its arguments, with no shell in between. Any word starting with `./` or `../` is in the plugin's folder.

- **Unseen (`place = "background"`, the default):** the program runs in the plugin's folder, without waiting. What it prints goes to its log (see [Where things are](#where-things-are)).
- **On screen (`popup`, `tab`, `split`, `split-down`):** it runs in a pane, in the folder of the pane you're in, like any program you type. A popup closes when the program ends.

What was written after the command's name (`lightsout do my-plugin.show today`) comes as `LIGHTSOUT_ARG`.

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
| `LIGHTSOUT_COMMAND`, `LIGHTSOUT_ARG` | for a command: its name, and what was written after it |
| `LIGHTSOUT_EVENT`, `LIGHTSOUT_EVENT_JSON` | for an event: its name, and what it carries |

## Keys

Your own keys come first, then plugins' keys (in the order the plugins were added), then Lights Out's.

- A plugin whose key is already used, by your settings, another plugin or Lights Out, **isn't installed or linked**. The message says who has the key and how to fix it: pick another key, or free a Lights Out key in your settings with `"ctrl+shift+t" = ""`.
- If you take a plugin's key later in your settings, yours wins, and a popup says once which plugin key now does nothing.

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

Plugins run only when something calls them: a command, a key, an event. Nothing checks on a timer, so a plugin costs nothing while nothing happens, also in Lights Out mode. A plugin that starts its own long-running program is that program's responsibility: keep it light.

## Trust

A plugin runs as you, like any program you install, and can use every Lights Out command. Install plugins you trust, and read what the install shows. Repos tagged `lightsout-plugin` are listed at [github.com/topics/lightsout-plugin](https://github.com/topics/lightsout-plugin): a list, not a review.

## Publishing

Put your plugin on GitHub (at the top of the repo, or in a folder), and tag the repo `lightsout-plugin` so people can find it. Users install it with `lightsout plugin install YOU/REPO`.
