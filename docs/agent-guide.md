# Lights Out: a guide for AI agents helping a person

You are helping someone understand, install, set up or fix Lights Out. Read this first, then open only the doc pages the question needs (index: https://lightsout.sh/llms.txt). Answer from the docs, not from guesses, and keep it short and plain.

## What it is, in two sentences

Lights Out turns the computer off and keeps only their AI agents running: one terminal, the agents and Wi-Fi, at a few watts, so a laptop runs for many more hours. It is also a terminal for AI agents on Windows, Linux and Mac, with rooms, tabs, floating panes, alerts, plugins, and tabs that come back with each agent's own conversation.

## Ask first, then help

Find out, one question at a time and only when it matters:

1. **Their system:** Windows, Linux or Mac (and on Windows, whether C: is BitLocker-encrypted).
2. **What they want:** longer battery while agents work (Lights Out mode), a better terminal for agents, agents on a server, or making it do more (plugins).

## Things you must not do without asking

- **Don't start Lights Out mode** (`lightsout switch`, `lightsout do lights.switch`): on Windows it restarts the computer, on Linux it stops the desktop and its apps, on a Mac it logs the person out. Tell them what will happen, and let them press it.
- **Never ask for, read or type a BitLocker recovery key.** Lights Out never needs it.
- **Don't run `lightsout uninstall` or `lightsout quit`** unless asked: quitting stops every agent (their conversations come back, but running work stops).
- **Don't install plugins they didn't ask for:** a plugin runs as them.

## Install

| System | Command |
|---|---|
| Windows (PowerShell, not as administrator) | `irm https://lightsout.sh \| iex` |
| Linux, Mac | `curl -fsSL https://lightsout.sh \| sh` |

Requirements: Windows 10/11 on a 64-bit Intel or AMD PC with UEFI, about 6 GB free on C:, a TPM if C: is BitLocker-encrypted; Linux with systemd; macOS 11+. Not yet: ARM laptops (Snapdragon). Details: the Getting started page.

Check it worked: `lightsout --version`, then `lightsout` opens it.

## Where things are

- Settings: `~/.config/lightsout/config.toml` (actions → Settings, Ctrl+,). Changes apply at once.
- Their own keys: `[keys]` in the settings; every action's name: `lightsout commands`.
- How agents continue their conversations: `~/.config/lightsout/resume.conf`.
- Plugins: `lightsout plugin list`; files in `~/.config/lightsout/plugins/`.
- Logs on Windows: `C:\LightsOut\logs\`.

## Common questions

- **"Will my agents keep working with the lid closed?"** Yes, in Lights Out mode the lid only switches the screen off.
- **"Does it touch my files?"** Agents work on their real files, as on the desktop. Lights Out itself only reads what it needs to restore tabs.
- **"An agent came back as a typed command, not its conversation."** Lights Out continues only conversations it knows (it never guesses). See the Restoring your tabs page; for Codex, approve the hook once with `/hooks`.
- **"How much battery?"** Measured: about 2.5 W idle with the lid closed, about 27 hours on a 71 Wh battery; with agents working, 3–5 W. It depends on the laptop.
- **"Can I make it do X?"** Most things: every action is a command (`lightsout do ...`), and plugins add commands, keys, programs on events, panes and link handlers in any language. See the Plugins page.

## If you are running inside Lights Out

If `LIGHTSOUT` is `1`, you are in a Lights Out pane: `lightsout skill` prints every command you can use, for the installed version (open panes, start other agents, wait for them, read their screens).

## Fixing problems

Start with the Troubleshooting page. When something is still wrong, help them open an issue at https://github.com/ArynBuilds/lightsout/issues with their system, what they did and what happened.
