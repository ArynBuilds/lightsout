# Commands

Everything Lights Out does can be done from any terminal, a script or an agent, with the `lightsout` command. Inside Lights Out it reaches the Lights Out you're in; elsewhere, your usual one (`--session NAME` picks another).

## The app

| Command | Does |
|---|---|
| `lightsout` | Runs Lights Out in this terminal (or brings back the one running in the background) |
| `lightsout --window` | Opens it in its own window |
| `lightsout --session NAME` | A separate set of tabs and rooms |
| `lightsout open USER@HOST` | Lights Out on a server, over SSH; its agents live there |
| `lightsout update` | Installs the newest version; settings, tabs, logins and programs stay ([Update](update.md)) |
| `lightsout uninstall` | Removes Lights Out ([Uninstall](uninstall.md)) |
| `lightsout --version` | The installed version |
| `lightsout skill` | Prints the skill: every command, written for agents |

## Panes

| Command | Does |
|---|---|
| `lightsout list [--json]` | Every pane: id, room, tab, state, title, folder (`--json`: everything, a floating pane's place and size too) |
| `lightsout new [--right\|--down] [--cwd DIR] [-- COMMAND...]` | Opens a pane (a new tab unless `--right`/`--down`) and prints its id; the command runs at once |
| `lightsout send PANE TEXT... [--no-enter]` | Types into a pane, and presses Enter unless `--no-enter` |
| `lightsout read PANE [--lines N]` | Prints what a pane shows (the last 60 lines unless `--lines`) |
| `lightsout wait PANE [--until idle\|done\|needs-you] [--timeout S]` | Waits until the pane gets there (default: done or idle; 600 s) |
| `lightsout explain PANE` | Why Lights Out thinks a pane is in its state |
| `lightsout focus PANE` | Brings a pane to the front |
| `lightsout close PANE` | Closes a pane and stops its program |

`PANE` is a number from `lightsout list`, or `self` for the pane the command runs in.

## Rooms

| Command | Does |
|---|---|
| `lightsout room add [NAME]` | A new room on this computer |
| `lightsout room add USER@HOST [NAME]` | A room that lives on a server (Lights Out must be installed there) |
| `lightsout room add NAME --on SSH-NAME` | The same, with a name from `~/.ssh/config` |
| `lightsout room list` | Rooms here, and the server of each server room |
| `lightsout room remove NAME` | Here: closes it and stops its agents. A server room: leaves it running there |

## Other

| Command | Does |
|---|---|
| `lightsout notify TEXT...` | A popup in Lights Out |
| `lightsout screen off\|on` | The screen off (saves battery) or on |
| `lightsout power` | Battery, time left and power draw |
| `lightsout switch` | Lights Out, or Lights On inside Lights Out, at once (no question) |
| `lightsout quit` | Saves the tabs and stops everything |
| `lightsout report-session ID` | For an agent: its conversation in this tab ([Add Lights Out support](add-lightsout-support.md)) |
| `lightsout plugin ...` | Install, link, list, update, remove, switch on or off plugins ([Plugins](plugins.md)) |

## Every action: `lightsout do`

Every action in the menu, every key and every button is a named command. `lightsout do NAME` does it exactly as its key would; `lightsout commands` lists them all, with your current keys and your plugins' commands.

```sh
lightsout do room.rename
lightsout do tab.go 2
lightsout do run.split npm test
lightsout do pane.move 10 20
```

| Command | Does | Key |
|---|---|---|
| `actions.open` | The actions menu | Ctrl+Shift+P |
| `tab.new` | New tab | Ctrl+Shift+T |
| `agent.start` | Pick an agent to start in a new tab | Ctrl+Shift+A |
| `command.ask` | Ask for a command to run in a new tab | |
| `pane.split-right` | Split right | Ctrl+Shift+Right |
| `pane.split-down` | Split down | Ctrl+Shift+Down |
| `pane.focus-left` / `-right` / `-up` / `-down` | Move to the pane on that side | Alt+arrows |
| `pane.close` | Close pane | Ctrl+Shift+W |
| `agent.jump` | Jump to the agent that needs you | Ctrl+Shift+J |
| `tab.next` / `tab.previous` | Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| `tab.go NUMBER` | Go to a tab (1 = the first) | Alt+1 … Alt+9 |
| `pane.output-in-editor` | Open a pane's output in an editor | Ctrl+Shift+E |
| `edit.copy` / `edit.paste` | Copy the selection / paste | Ctrl+Shift+C / Ctrl+Shift+V, Shift+Insert |
| `pane.scroll-up` / `pane.scroll-down` | Scroll half a screen | Shift+PageUp / Shift+PageDown |
| `font.bigger` / `font.smaller` | Bigger / smaller text (window only) | Ctrl+= / Ctrl+- |
| `room.new` | New room | Ctrl+Shift+N |
| `room.rename` | Rename this room | |
| `room.close` | Delete this room (a server room: remove it here) | |
| `room.new-on-server` | New room on a server... | |
| `room.go NUMBER` | Go to a room (1 = the first) | |
| `sidebar.toggle` | Show / hide the sidebar | Ctrl+Shift+B |
| `settings.open` | Settings | Ctrl+, |
| `wifi.choose` | Wi-Fi networks | |
| `wifi.join NUMBER` / `wifi.other` | Join a network from the list / by its name | |
| `screen.off` | Screen off | Ctrl+Shift+O |
| `lights.switch` | Lights Out, or Lights On inside Lights Out (asks first) | Ctrl+Shift+L |
| `app.detach` | Close this view, agents keep running (inside a terminal) | Ctrl+B D |
| `app.quit` | Quit, stops all agents (in Lights Out: Lights On) | Ctrl+Shift+Q |
| `run.popup COMMAND` | A command line in a popup that closes when it ends | |
| `run.tab COMMAND` | A command line in a new tab | |
| `run.split COMMAND` | A command line in a split | |
| `PLUGIN.COMMAND [ARG]` | A [plugin's](plugins.md) command | its own |

### Floating panes

`PANE` is optional: without it, the pane you're in.

| Command | Does |
|---|---|
| `pane.float [PANE]` | The pane floats over the others (actions: Float this pane) |
| `pane.tile [PANE]` | Back into the layout |
| `pane.move X Y [PANE]` | Floats it with its top left corner at X, Y (% of the area, 0–100) |
| `pane.resize W H [PANE]` | Floats it at that size (% of the area, 5–100) |
| `pane.minimize [PANE]` | Out of sight until it is focused or raised |
| `pane.raise [PANE]` | To the front, and back from minimised |

With these and `lightsout list --json`, a script or a [plugin](plugins.md) can arrange panes any way it likes, like a window manager.
