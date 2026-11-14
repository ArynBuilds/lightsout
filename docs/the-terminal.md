# The terminal

Lights Out is a terminal made for running AI agents: everything you need is on screen, and everything else is one key away.

## The window

```text
┌──────────────┬──────────────────────────────────────┐
│ Room 1       │ ● 1 api   ○ 2 web   +                 │  ← tabs of this room
│ Room 2       ├──────────────────────────────────────┤
│ + room       │                                       │
│──────────────│  ~/api $ claude                       │
│ ◌ claude api │                                       │  ← the terminal
│ ● codex web  │                                       │
│              │                                       │
│ 84% 4.2 W    │                                       │
│ Lights Out   │                                       │
│   actions    │                                       │
└──────────────┴──────────────────────────────────────┘
```

- **Top half of the sidebar:** your rooms.
- **Bottom half:** what runs in the room you're in, one line each, with its state. Click one to go to it.
- **Bottom left:** the battery and power draw, **Lights Out** (or **Lights On** inside Lights Out) and **actions**.
- **Top:** the tabs of the room you're in.

The sidebar can be on the right and the tabs at the bottom, or either can be off: `[parts]` in [the settings](configuration.md#colours-and-the-look). [Plugins](plugins.md#parts-of-the-screen) can add parts on any side, or replace these two.

## Rooms

A room is its own set of tabs, for example one room per project. Click a room to switch to it; its tabs and agents come with it. Agents in other rooms keep running.

- **New room:** click **+ room** or press **Ctrl+Shift+N**.
- **Rename or delete a room:** open **actions**. Deleting a room stops its agents.
- **A room on a server:** **actions → New room on a server...** (or `lightsout room add me@server`). Its agents run on the server; the room shows them live, and keeps reconnecting if the connection drops (dimmed meanwhile). See [Rooms on a server](configuration.md#rooms-on-a-server).

## Tabs and splits

| Do | Keys |
|---|---|
| New tab | Ctrl+Shift+T |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Go to tab 1–9 | Alt+1 … Alt+9 |
| Split right | Ctrl+Shift+Right |
| Split down | Ctrl+Shift+Down |
| Move between panes | Alt+arrows |
| Close the pane | Ctrl+Shift+W |

Drag the line between two panes to resize them.

## Floating panes

A pane can float over the others like a window, with its own place and size: **actions → Float this pane**.

- **Move it:** drag its title bar.
- **Resize it:** drag its bottom right corner.
- **Title bar buttons:** `–` minimises it, `▢` puts it back into the layout, `×` closes it.
- Clicking a floating pane brings it to the front. A minimised one comes back when you go to it, for example by clicking it in the sidebar.
- Every pane of a tab can float, leaving an empty space behind the windows.

Floating panes keep their place, size and order across restarts and Lights Out. Programs and plugins can arrange them too (`pane.move`, `pane.resize`, `pane.minimize`...: see [Commands](commands.md#floating-panes)).

## Actions

Click **actions** at the bottom left, or press **Ctrl+Shift+P**, to see everything Lights Out can do, including what your [plugins](plugins.md) add. Type to filter, press Enter to run. Each action shows its key.

Every action is also a named command: `lightsout do room.rename` does from any terminal what the menu does. `lightsout commands` lists them all. See [Commands](commands.md).

## All keys

| Action | Keys |
|---|---|
| Actions | Ctrl+Shift+P |
| Start an agent | Ctrl+Shift+A |
| Jump to the agent that needs you | Ctrl+Shift+J |
| Lights Out / Lights On | Ctrl+Shift+L |
| Screen off (any key turns it on) | Ctrl+Shift+O |
| New room | Ctrl+Shift+N |
| Show / hide the sidebar | Ctrl+Shift+B |
| Open a pane's output in an editor | Ctrl+Shift+E |
| Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V (also Shift+Insert) |
| Scroll half a screen | Shift+PageUp / Shift+PageDown |
| Bigger / smaller text | Ctrl+= / Ctrl+- |
| Settings | Ctrl+, |
| Quit (stops all agents; in Lights Out: Lights On) | Ctrl+Shift+Q |
| Plugins off / on (the way back if one breaks the screen) | Ctrl+Shift+F12 |

Change any of them, give a key to any action, or free a key for your programs: see [Your own keys](configuration.md#your-own-keys). Plugins can add keys too.

## Mouse

- Click a tab, a room or an agent to go to it.
- Drag to select text; letting go copies it.
- Right-click for copy, paste, split, the editor and close.
- Scroll to see earlier output.
- **Ctrl+click a web link** to open it in your browser (or in a [plugin](plugins.md#links) that handles links like it).
- Programs that use the mouse (editors, `htop`, Lights Out on a server) get clicks and the wheel. Hold **Shift** to select text or open the menu instead.

## Alerts

When an agent finishes or needs your answer, a popup appears in the Lights Out window (never a system notification), and the agent's mark in the sidebar changes. A short sound can be switched on in [the settings](configuration.md). **Ctrl+Shift+J** jumps straight to the agent that needs you.

Only real work counts: an agent sitting at its prompt, redrawing its status line, never shows up as "done".

Lights Out recognises common questions by itself. If one of your tools asks in its own words, add the phrase to `needs_you` in [the settings](configuration.md#when-an-agent-asks-in-its-own-words).

## Closing and coming back

Closing the window doesn't stop your agents: they keep running, and `lightsout` brings the window back. **Quit** in actions stops everything. Your tabs come back the next time you open Lights Out, each in its folder, each agent with its own conversation: see [Restoring your tabs](session-state.md).

## Inside any terminal

Type `lightsout` in PowerShell, cmd, bash, zsh or any terminal app, and Lights Out runs right there, like tmux: same rooms, tabs and sidebar, in your terminal's font.

Terminals keep many Ctrl+Shift keys for themselves, so there the keys are **Ctrl+B, then the key**: Ctrl+B P actions, T new tab, W close, N new room, A start an agent, → split right, ↓ split down, Tab next tab. Ctrl+B twice sends Ctrl+B to the program. **F1** also opens actions.

Plugins' pictures (frames, such as a web page) show there too in terminals that can draw them: kitty, WezTerm, Ghostty, iTerm2 and Windows Terminal.

Closing that terminal doesn't stop anything: your agents keep running in the background, and `lightsout` brings you back to the same screen. **Ctrl+B D** (or **actions → Detach**) does the same on purpose. `lightsout --window` opens the window instead.

## Wi-Fi

**actions → Wi-Fi...** lists nearby networks. Pick one and type the password. **Other network...** joins a hidden network by name.

## Several separate sets of tabs

```sh
lightsout --session work
```

opens a separate Lights Out with its own tabs, rooms and saved state. Commands for it need the same `--session work` (or `LIGHTSOUT_SESSION=work`).
