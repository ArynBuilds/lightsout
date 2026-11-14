# Agents

Lights Out runs any command-line agent or tool: Claude Code, Codex, Gemini CLI, OpenCode, Aider, Copilot CLI, Cursor's agent, or plain commands like `npm test`. Nothing is bundled: whatever runs in a pane is what Lights Out works with.

## Installing an agent

Install agents the way their makers describe, inside Lights Out or before. In Lights Out mode on Windows, `lightsout-install-agents` offers the common ones. Your logins are kept: you sign in once.

## Their conversations come back

When Lights Out reopens your tabs (after Quit, a restart, Lights Out or Lights On), each agent continues its own conversation, with the options it was started with: two Claudes in one folder each get theirs back. Claude Code, Codex and OpenCode work this way out of the box; any other agent can be added with a few lines. See [Restoring your tabs](session-state.md).

## States

Lights Out reads each pane's screen to tell what the program is doing, for any program:

| Mark | State | Means |
|---|---|---|
| `●` | needs you | asks a question, waits for a yes or a password |
| `◌` | working | output is changing |
| `○` | done | it worked for a while and stopped |
| `·` | idle | nothing happening |

`lightsout explain <pane>` tells you why a pane is in its state.

## One agent runs others

Every pane can be driven with `lightsout` commands, from any terminal or from an agent: open panes, type into them, read them, wait for them.

```sh
id=$(lightsout new --cwd ~/web -- codex)
lightsout send "$id" "Fix the failing test in checkout.spec.ts"
lightsout wait "$id" --until done --timeout 1800
lightsout read "$id" --lines 40
```

If `wait` ends with `needs-you`, the agent asks something: read the pane, then answer with `send`, or tell the user with `lightsout notify`.

Every program in a pane knows its own pane from `$LIGHTSOUT_PANE` (and `self` stands for it: `lightsout read self`). All commands: [Commands](commands.md).

## Teaching your agent

Lights Out comes with a skill: a short file that tells an agent every command it can run (panes, rooms, servers, waiting for other agents, plugins, the screen). Agents use it only inside Lights Out, where `LIGHTSOUT` is `1`.

```sh
npx skills add ArynBuilds/lightsout     # Claude Code, Codex, Cursor and others
lightsout skill                         # or print it (it matches your installed version)
```

Agents without a skill system: add the output of `lightsout skill` to their instructions (for example `CLAUDE.md` or `AGENTS.md`). For agents reading the web, https://lightsout.sh/llms.txt indexes these docs, and https://lightsout.sh/llms-full.txt has them all in one file.

## Security of the control commands

The commands talk to the app over a local connection on `127.0.0.1` with a random token that only your user account can read. Nothing listens on your network.
