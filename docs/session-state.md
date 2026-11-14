# Restoring your tabs

Whenever Lights Out closes and opens again, everything comes back as it was: rooms, tabs, folders, and each agent with its own conversation.

## When

- You **Quit** and open Lights Out again, or the computer restarted, or the app crashed.
- You press **Lights Out**: the tabs open again in Lights Out mode.
- You press **Lights On**: the tabs come back to Windows, your Linux desktop, or your Mac when you sign in.
- You run `lightsout update`.

## What comes back

| What | How |
|---|---|
| Rooms | their names, their order, the one you were in |
| Tabs and splits | the same layout and sizes, the same tab in front |
| Floating panes | their place, size and order, minimised or not |
| Folders | each pane opens in the folder its program works in **now** (an agent that changed folder comes back in the new one) |
| Agents | each continues **its own** conversation, with the options it was started with (below) |
| Other programs | typed back in, **not run**, so you decide (a dev server, `npm test`...) |
| Screens | a pane's last screen is shown dimmed above the new prompt (not for agents, which draw their conversation again) |
| Rooms on a server | they reconnect; their tabs never left the server |

Your tabs are saved within two seconds of any change (a room renamed, a tab opened or closed, a program started or ended, a folder changed), and the screens once a minute while agents work. So even a crash or a forced shutdown loses nothing.

To start fresh instead after Quit or a restart, set `keep = false` under `[history]` in [the settings](configuration.md). Lights Out and Lights On still bring your tabs.

## How each agent continues

Lights Out never guesses. It continues an agent only when it knows that pane's own conversation; "the latest conversation in this folder" might be another tab's. Without it, the command is typed back in, unrun, like any other program.

Out of the box:

| Agent | Comes back as | How Lights Out knows the conversation |
|---|---|---|
| Claude Code | `claude [its options] --resume <its chat>` | Claude writes it in `~/.claude/sessions/` while it runs; read at every save, so a chat switched with `/resume` comes back too |
| Codex | `codex [its options] resume <its chat>` | Codex tells it through a hook Lights Out adds to `~/.codex/hooks.json` |
| OpenCode | `opencode [its options] --session <its chat>` | when it reports it (see below) |
| Aider | `aider --restore-chat-history` | one history per folder |

The options it was started with come back (`codex --yolo` stays `--yolo`); options that pick a conversation (`--resume`, `--continue`, `--last`...) are never given again.

> **Codex:** the first time, Codex asks you to approve the new hook (type `/hooks` in Codex and allow `lightsout report-session`). It prints nothing and costs no tokens. Until then, Codex tabs are typed back in.

## Adding an agent

Everything above is plain data in `resume.conf`. Your own lines go in `~/.config/lightsout/resume.conf` and win over the built-in ones:

```ini
#   PROGRAM = COMMAND          continues that exact conversation: {id} is its ID, {args} the
#                              options it was started with
#   PROGRAM record = FILE KEY  where the agent writes its conversation ID while it runs
#                              ({pid}: its process number)
#   PROGRAM hook = FILE        its hooks file: Lights Out adds `lightsout report-session`
#   PROGRAM skip = OPTIONS     its own conversation options, never given again

claude        = claude {args} --resume {id}
claude record = ~/.claude/sessions/{pid}.json sessionId
claude skip   = --resume -r --continue -c --fork-session --print -p
codex         = codex {args} resume {id}
codex hook    = ~/.codex/hooks.json
codex skip    = --last
opencode      = opencode {args} --session {id}
opencode skip = --continue -c --session -s
aider         = aider --restore-chat-history

# your own:
mytool        = mytool {args} --resume {id}
mytool record = ~/.mytool/running/{pid}.json conversation_id
```

An agent with one history per folder can have a command without `{id}`. Making an agent? See [Add Lights Out support to your agent](add-lightsout-support.md): one call is enough.
