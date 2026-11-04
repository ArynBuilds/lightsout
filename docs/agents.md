# Agents

Lights Out runs any command-line agent or tool: Claude Code, Codex, Gemini CLI, OpenCode, Aider, Copilot CLI, Cursor's agent, or plain commands like `npm test`. Nothing is bundled and nothing is hard-coded: whatever runs in a tab is what Lights Out works with.

## Installing an agent

Install agents the way their makers describe, inside Lights Out or before. In Lights Out mode on Windows, `lightsout-install-agents` offers the common ones. Your logins are kept: you sign in once.

## How conversations continue

When you switch to or from Lights Out, each tab is handed over with its folder and the command it ran. Agents that can continue a conversation are started that way:

| Agent | Continues with |
|---|---|
| `claude` | `claude --continue` |
| `codex` | `codex resume --last` |
| `opencode` | `opencode --continue` |
| `aider` | `aider --restore-chat-history` |

Any other command is typed back into its tab, without being run, so you decide. Add your own in `~/.config/lightsout/resume.conf`:

```ini
# program = the command that continues it
mytool = mytool --resume
```

## States

Lights Out reads each pane's screen to tell what the program is doing, for any program:

| Mark | State | Means |
|---|---|---|
| `●` | needs you | asks a question, waits for a yes or a password |
| `◌` | working | output is changing |
| `○` | done | it worked for a while and stopped |
| `·` | idle | nothing happening |

`lightsout explain <pane>` tells you why a pane is in its state.

## Control from the command line

Every pane can be driven with `lightsout` commands, from any terminal or from an agent. That's how an agent runs other agents.

| Command | Does |
|---|---|
| `lightsout list` | Every pane: id, room, tab, state, title and folder |
| `lightsout new [--right\|--down] [--cwd DIR] [-- COMMAND...]` | Opens a pane (a new tab unless `--right`/`--down`) and prints its id |
| `lightsout send PANE TEXT... [--no-enter]` | Types into a pane, and presses Enter unless `--no-enter` |
| `lightsout read PANE [--lines N]` | Prints what a pane shows (the last 60 lines unless `--lines`) |
| `lightsout wait PANE [--until idle\|done\|needs-you] [--timeout S]` | Waits until the pane reaches that state |
| `lightsout explain PANE` | Why Lights Out thinks a pane is in its state |
| `lightsout notify TEXT...` | Shows a popup |
| `lightsout screen off\|on` | Switches the screen off or on |
| `lightsout power` | Battery and power draw |
| `lightsout skill` | Prints the skill: every command, written for agents |
| `lightsout room add/list/remove` | Rooms here and rooms on a server |
| `lightsout open me@server` | Lights Out on a server, in this terminal |
| `lightsout uninstall` | Removes Lights Out |

Every program in a pane knows its own pane id from `$LIGHTSOUT_PANE`.

### Example: one agent hands work to another

```sh
id=$(lightsout new --cwd ~/web -- codex)
lightsout send "$id" "Fix the failing test in checkout.spec.ts"
lightsout wait "$id" --until done --timeout 1800
lightsout read "$id" --lines 40
```

### Teaching your agent

Lights Out comes with a skill: a short file that tells an agent every command it can run (panes, rooms, servers, waiting for other agents, the screen). Agents only use it inside Lights Out.

```sh
npx skills add ArynBuilds/lightsout     # Claude Code, Codex, Cursor and others
lightsout skill                         # or print it (it matches your installed version)
```

Agents without a skill system: add the output of `lightsout skill` to their instructions (for example `CLAUDE.md` or `AGENTS.md`). For agents reading the web, https://lightsout.sh/llms.txt indexes these docs, and https://lightsout.sh/llms-full.txt has them all in one file.

## Security of the control commands

The commands talk to the app over a local connection on `127.0.0.1` with a random token that only your user account can read. Nothing listens on your network.
