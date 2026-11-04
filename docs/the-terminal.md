# The terminal

Lights Out is a terminal made for running AI agents: everything you need is on screen, and everything else is one shortcut away.

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
│ 84%          │                                       │
│ Lights Out   │                                       │
│   actions    │                                       │
└──────────────┴──────────────────────────────────────┘
```

- **Top half of the sidebar:** your rooms.
- **Bottom half:** the programs running in the room you're in, one line each. Click one to go to it.
- **Bottom left:** the battery, **Lights Out** (or **Lights On** inside Lights Out) and **actions**.
- **Top:** the tabs of the current room.

## Rooms

A room is its own set of tabs, for example one room per project. Click a room to switch to it; its tabs and agents come with it. Agents in other rooms keep running.

- **New room:** click **+ room** or press **Ctrl+Shift+N**.
- **Rename or close a room:** open **actions**. Closing a room stops its agents.

## Tabs and splits

| Do | Keys |
|---|---|
| New tab | Ctrl+Shift+T |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Split right | Ctrl+Shift+Right |
| Split down | Ctrl+Shift+Down |
| Move between panes | Alt+arrows |
| Close the pane | Ctrl+Shift+W |

Drag the line between two panes to resize them.

## Actions

Click **actions** at the bottom left, or press **Ctrl+Shift+P**, to see everything Lights Out can do. Type to filter, press Enter to run. Each action shows its shortcut.

## All shortcuts

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
| Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V |
| Bigger / smaller text | Ctrl+= / Ctrl+- |
| Settings | Ctrl+, |
| Quit (stops all agents) | Ctrl+Shift+Q |

You can add your own keys: see [Configuration](configuration.md#your-own-keys).

## Mouse

- Click a tab, a room or an agent to go to it.
- Drag to select text; it's copied with Ctrl+Shift+C.
- Right-click for copy, paste, split and close.
- Scroll to see earlier output.

## Alerts

When an agent finishes or needs your answer, Lights Out shows a popup and plays a short sound (Windows and Mac). **Ctrl+Shift+J** jumps straight to the agent that needs you.

Lights Out recognises common questions by itself. If one of your tools asks in its own words, add the phrase to `needs_you` in [the settings](configuration.md).

## Closing the window

Closing the window doesn't stop your agents: they keep running, and `lightsout` brings the window back. **Quit** in actions stops everything. Your tabs come back the next time you open Lights Out.

## Wi-Fi

**actions → Wi-Fi...** lists nearby networks. Pick one and type the password. **Other network...** joins a hidden network by name.

## Several separate sets of tabs

```sh
lightsout --session work
```

opens a separate window with its own tabs, rooms and saved state.
