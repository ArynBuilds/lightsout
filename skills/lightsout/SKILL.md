---
name: lightsout
description: Work inside Lights Out, the terminal for AI agents. Open panes, tabs and rooms; start other agents and give them tasks; read their screens and wait for them; use rooms that live on a server; switch the screen off to save battery. Only when the LIGHTSOUT environment variable is 1 (you are in a Lights Out pane).
---

# Lights Out

Lights Out is a terminal for AI agents. Rooms hold tabs, tabs hold panes, and every
pane runs a shell or an agent. Some rooms live on a server: their agents run there.

## First check

These commands are only for programs running inside Lights Out. Check that
`LIGHTSOUT` is `1` (`echo $LIGHTSOUT`; in PowerShell `$env:LIGHTSOUT`). If it isn't,
you are not in Lights Out: don't use them. Your own pane's id is in `LIGHTSOUT_PANE`.

## Panes

```
lightsout list                              every pane: id, room, tab, state, title, folder
lightsout new [--right|--down] [--cwd DIR] [-- COMMAND...]
                                            open a pane (a new tab unless --right/--down) and print its id
lightsout send PANE TEXT... [--no-enter]    type into a pane (Enter is pressed unless --no-enter)
lightsout read PANE [--lines N]             what a pane shows (the last N lines, 60 by default)
lightsout wait PANE [--until idle|done|needs-you] [--timeout SECONDS]
                                            wait until the pane gets there (default: done or idle)
lightsout explain PANE                      why Lights Out thinks a pane is in its state
lightsout close PANE                        close a pane and stop its program (like "Close pane")
```

States: `working` (output is changing), `needs-you` (it asks a question),
`done` (it finished while nobody was looking), `idle`.

## Rooms

```
lightsout room list                         rooms here, and the server each server room is on
lightsout room add [NAME]                   a new room on this computer
lightsout room add USER@HOST [NAME]         a room that lives on a server (Lights Out must be
                                            installed there); `NAME --on SSH-NAME` for ~/.ssh/config names
lightsout room remove NAME                  a room here: closes it (stops its agents);
                                            a server room: leaves it running on the server
```

## Everything else Lights Out can do

Every action, menu line and key is a named command:

```
lightsout commands                          all of them, with their keys
lightsout do COMMAND [ARG]                  do one, as its key or the menu does
                                            (`lightsout do room.rename`, `lightsout do tab.go 2`,
                                            `lightsout do run.split npm test`)
```

## Plugins

A plugin is a folder with `lightsout-plugin.toml` and programs in any language. It adds
commands (they join `lightsout commands` as `PLUGIN.COMMAND`), keys, and programs that run
on events. Its programs get `LIGHTSOUT_PLUGIN_CONTEXT` (room, tab, pane, folder, program,
selected text as JSON), `LIGHTSOUT_ARG` (what was written after the command's name),
`LIGHTSOUT_EVENT` and `LIGHTSOUT_EVENT_JSON` (on an event), and talk back with these same
`lightsout` commands.

```toml
id = "my-plugin"
name = "My plugin"
version = "0.1.0"
[[commands]]
id = "hello"                                # lightsout do my-plugin.hello
title = "Say hello"                         # its line in the actions menu
run = ["sh", "./hello.sh"]                  # ./ = the plugin's folder
place = "popup"                             # on screen: popup, tab, split, split-down
                                            # (default: background, unseen)
[[keys]]
key = "ctrl+alt+h"
command = "my-plugin.hello"
[[events]]
on = "agent.needs-you"                      # all of them: lightsout plugin events
run = ["sh", "./ping.sh"]
[[links]]                                   # Ctrl+click on a matching link (* = anything)
pattern = "https://github.com/*/pull/*"     # runs the command with the link
command = "my-plugin.hello"                 # (no match: the link opens in the browser)
```

Changes to plugins apply without restarting Lights Out.

```
lightsout plugin link FOLDER                use a plugin you are making
lightsout plugin install OWNER/REPO[/FOLDER] [--ref TAG] [--yes]
lightsout plugin list | update | remove ID | on ID | off ID | events
```

Install only what the user asked for: a plugin runs as the user.

## Other

```
lightsout notify TEXT...                    a popup for the user
lightsout screen off|on                     the screen off (saves battery) or back on
lightsout power                             battery and current power draw
lightsout report-session ID                 your own conversation's ID, so your tab comes back to it
                                            (also reads a hook's JSON on stdin; prints nothing)
lightsout skill                             prints this file
```

## Working with other agents

1. Start one: `lightsout new -- claude` (or `codex`, `gemini`, `opencode`, any CLI). It prints the pane id.
2. Give it a task: `lightsout send <id> "Fix the failing test in src/parser"`.
3. Wait: `lightsout wait <id> --until done --timeout 1800`.
4. Read its answer: `lightsout read <id> --lines 120`.

If `wait` ends with `needs-you`, read the pane: the agent asks something. Answer with
`send`, or tell the user with `notify`.

## Good manners

- Use your own room and tabs; don't close or type into panes you didn't open unless the user asks.
- When you are done and the user is away, `lightsout screen off` saves battery.
- More: https://lightsout.sh/llms.txt
