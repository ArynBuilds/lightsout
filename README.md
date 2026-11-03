# Lights Out

**Close your laptop. Your AI agents keep working, on a fraction of the power.**

Coding agents like Claude Code, Codex or Aider hardly use your computer. The model runs in the cloud, and locally the agent mostly waits on the network, edits files and runs git. Yet while they work, your laptop keeps its bright screen, turbo boost and every background app running. And if you close the lid, Windows puts it to sleep and your agent stops.

Lights Out fixes both with one command.

```
lightsout run claude        # start your agent (Claude Code gets phone control)
lightsout dark              # agent mode: close the lid and walk away
```

## What `lightsout dark` does

- **Screen off.** Close the lid and it keeps running.
- **Turbo boost off**, and the CPU speed is capped.
- **Windows Energy Saver on.**
- **Every background app goes into Efficiency mode** (the leaf icon in Task Manager), so they run on efficient cores at low speed. Your agents don't.
- **Bluetooth off.**
- **It finds the power hogs.** Before going dark it shows which apps are burning your battery (a stuck browser tab, Docker, a game launcher) so you can close them with `--close`.
- **It measures your power draw** before and during, then reports what you saved.

When you end it (Ctrl+C, `lightsout wake`, or automatically when your agents finish), **everything goes back exactly as it was** and the screen wakes up. Every original setting is saved before it's changed. Closing the window restores everything too, and if the PC crashes or the battery dies mid-run, Lights Out restores your settings automatically at the next sign-in. `lightsout restore` does it by hand.

```
LIGHTS OUT  (balanced)

Measuring normal power draw ...... 15.2 W

Using power right now:
  Docker Desktop             95% of a CPU core
Closing these saves the most. Example: lightsout dark --close "Docker Desktop"

  ✓ closing the lid keeps it running
  ✓ turbo boost off
  ✓ CPU speed capped
  ✓ Energy Saver on
  ✓ screen sleeps after 30 s
  ✓ 201 background processes in Efficiency mode
```

## Real numbers

ASUS ROG Zephyrus G16 (Core Ultra 9 185H), on battery, one agent running, 5 minutes, lid closed:

| | Power draw | Full battery lasts |
|---|---|---|
| Normal | 11.7 W | 6 h 06 m |
| Lights Out | **5.4 W** (settles at ~4.8 W) | **13 h 04 m** |

Internet stayed up the whole time (330/330 pings answered). Before that run, Lights Out flagged Docker Desktop burning a full CPU core in the background; with it running, the same laptop drew 20–30 W.

One laptop, one short test: your numbers will differ. `lightsout dark` measures your own before and after.

## Control it from your phone

`lightsout run claude` starts Claude Code with **Remote Control** on, so you can watch and steer the session from the Claude app on your phone while the laptop sits closed in your bag. Any other CLI agent works too: `lightsout run codex`, `lightsout run aider`, …

From the phone you can even end agent mode: ask the agent to run `lightsout wake`.

## Install

Download `lightsout.exe` from [Releases](../../releases) and put it somewhere on your PATH. One 400 KB file, no installer, no admin rights needed.

Version 0.1 isn't code-signed yet, so Windows may show a SmartScreen warning the first time (More info → Run anyway). Signing is coming.

Or build it: `cargo install --git https://github.com/ArynBuilds/lightsout`

## Commands

| Command | What it does |
|---|---|
| `lightsout run <cmd> [args]` | Start an agent. Keeps the PC awake while it runs. |
| `lightsout dark` | Agent mode (see above). |
| `lightsout status` | Power draw, battery, running agents. |
| `lightsout wake` | End agent mode from any terminal. |
| `lightsout restore` | Undo every change if something was interrupted. |

`dark` options:

| Option | |
|---|---|
| `--eco` | Deeper savings: lower CPU cap, agents in Efficiency mode too. |
| `--until-done` | End agent mode when all agents have finished. |
| `--then-sleep` | With `--until-done`: sleep the laptop afterwards. |
| `--screen` | Keep the screen on. |
| `--keep-bluetooth` | Leave Bluetooth on (e.g. for a Bluetooth mouse). |
| `--close chrome,discord` | Politely close these apps first. |

## Good to know

- Measuring power needs a battery and the charger unplugged.
- Windows may lock when the screen turns off. That's your Windows sign-in setting; Lights Out leaves it alone.
- Lights Out never forces the screen off. On many laptops that triggers standby, which freezes your agents and can cut Wi-Fi. Windows turns the screen off after 30 s idle instead, while Lights Out keeps the system awake.
- Windows only. Next up: a mode where Windows shuts off completely and a tiny Linux keeps your agents running.
- Lights Out is independent and not affiliated with Anthropic or any agent vendor.

## License

MIT
