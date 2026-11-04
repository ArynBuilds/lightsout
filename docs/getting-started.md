# Getting started

Install Lights Out with one line, open it, start an agent, and press **Lights Out**.

## Requirements

| System | Needs |
|---|---|
| Windows | Windows 10 or 11, a 64-bit Intel or AMD PC that starts with UEFI (most PCs from 2012 on), about 6 GB free on C:. A TPM security chip if C: is encrypted with BitLocker. |
| Linux | A 64-bit Intel or AMD PC with systemd. Lights Out mode installs `sway` if it is missing. |
| Mac | macOS 11 or newer, Apple chip or Intel. |

Not supported yet: Snapdragon and other ARM laptops.

## Install

**Windows.** Open PowerShell (no need to run it as administrator) and paste:

```powershell
irm https://lightsout.sh | iex
```

Windows asks once for admin rights. The installer checks that your PC can run Lights Out before it changes anything, then asks you to choose a PIN for the Lights Out lock screen.

**Linux and Mac.** Open a terminal and paste:

```sh
curl -fsSL https://lightsout.sh | sh
```

It asks for your password once (sudo).

> **Note:** The installer downloads everything from the [latest release](https://github.com/ArynBuilds/lightsout/releases/latest) on GitHub. To read it first, open https://lightsout.sh/install.ps1 or https://lightsout.sh/install.sh in your browser.

## Open it

```sh
lightsout
```

On Windows it's also in the Start menu, on a Mac in Launchpad, and on Linux in your apps menu. It opens a normal terminal with your own shell.

## Start an agent

Type `claude`, `codex` or any other command, as in any terminal. Or press **Ctrl+Shift+A** to pick an agent from a list.

Agents show up in the sidebar with a mark that tells you their state at a glance:

| Mark | Means |
|---|---|
| `●` | It needs you: it asks a question or waits for a yes |
| `◌` | It's working |
| `○` | It's done |
| `·` | Idle |

## Switch to Lights Out

Press **Lights Out** at the bottom left of the sidebar, or **Ctrl+Shift+L**, and confirm.

- **Windows** saves your tabs and restarts into Lights Out: Windows is fully off, and the same terminal comes back with your tabs. Conversations continue where they were.
- **Linux** stops the desktop, with no restart. Only Lights Out runs.
- **Mac** moves your agents into a small background service and logs you out: only macOS’s core and your agents keep running. Apps with unsaved work ask first.

Your agents keep working on your real files. Close the lid or press **Ctrl+Shift+O** to switch the screen off.

## Come back

Press **Lights On** at the bottom left (or **Ctrl+Shift+L**): you're back in Windows or on your Linux desktop, with your tabs. On a Mac, just sign in: Lights Out opens and takes your tabs back.

## Next

- [The terminal](the-terminal.md): tabs, rooms, splits and every shortcut
- [Lights Out mode](lights-out-mode.md): exactly what happens on each system
- [Agents](agents.md): how agents continue, and how agents can run agents
