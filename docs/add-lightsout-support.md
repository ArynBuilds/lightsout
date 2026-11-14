# Add Lights Out support to your agent

When Lights Out reopens its tabs, it continues each agent's own conversation, but only when it knows which one that pane had. If you make a command-line agent, tell Lights Out the conversation, and your users' tabs come back exactly. No change to Lights Out is needed.

## 1. Tell Lights Out the conversation

Inside Lights Out, every pane has `LIGHTSOUT=1` and `LIGHTSOUT_PANE` set. Whenever a conversation starts (a new one, or one the user switched to), run:

```sh
lightsout report-session <conversation-id>
```

It prints nothing, never fails (it exits 0 even outside Lights Out), and costs nothing. It can also read the ID from JSON on stdin, so it fits hook systems as it is: the first of `session_id`, `sessionId`, `conversation_id` or `thread_id` is used.

```sh
echo '{"session_id":"abc123"}' | lightsout report-session
```

The other way: write the ID to a file while your agent runs, named after its process number (as Claude Code does in `~/.claude/sessions/<pid>.json`), and add a `record` line (step 2). Lights Out reads it at every save.

## 2. Say how to continue it

One line in `resume.conf`: the command that continues a conversation, with `{id}` for its ID and `{args}` for the options the user started your agent with:

```ini
mytool      = mytool {args} --resume {id}
mytool skip = --resume --continue          # your own options that pick a conversation
mytool record = ~/.mytool/sessions/{pid}.json id   # only for the file way
mytool hook = ~/.mytool/hooks.json          # only if your agent has a hooks file Lights Out may add to
```

Users can add these lines to `~/.config/lightsout/resume.conf` today. To make it built in for everyone, send a pull request adding them to [`resume.conf`](https://github.com/ArynBuilds/lightsout/blob/main/os/overlay/usr/local/share/lightsout/resume.conf).

## What your users get

- After Quit, a restart, Lights Out or Lights On, each tab with your agent comes back in its folder as `mytool <their options> --resume <its own conversation>`.
- Two tabs of your agent in one folder each get their own conversation back.
- Without a reported ID nothing is guessed: the command is typed back in, unrun.

See [Restoring your tabs](session-state.md) for everything that comes back.
