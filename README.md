# Lights Out

**Turn your computer off. Keep your AI agents running.**

Press **Lights Out** and Windows shuts down completely. A tiny Linux starts in its place and runs only one thing: the Lights Out terminal with your agents (Claude Code, Codex, Gemini CLI, OpenCode, Aider, or any command). Everything else is off, so the laptop runs for many more hours on battery. Press **Lights On** and you're back in Windows. Your tabs, folders and agent conversations come along both ways.

## Install

Windows (PowerShell):

```powershell
irm https://lightsout.sh | iex
```

Linux:

```sh
curl -fsSL https://lightsout.sh | sh
```

Then open it with `lightsout`. Remove it any time with `lightsout uninstall`.

## What it does

- **One terminal everywhere.** The same app on Windows, on Linux, and in Lights Out mode: tabs, splits, rooms, mouse, copy/paste, alerts when an agent needs you.
- **Lights Out (Windows).** Windows shuts down fully. The small Linux unlocks your drive with the TPM (no recovery key to type), joins your Wi-Fi, and reopens your tabs. Agents work on your real files with your existing logins.
- **Lights Out (Linux).** No restart: the desktop stops, only Lights Out runs, and Lights On brings the desktop back.
- **What's off:** all of Windows, the extra graphics card, Bluetooth, sound, the keyboard light, the screen (turn it off, or close the lid). Work runs on the 1–2 most frugal CPU cores with turbo off and a power cap; idle chips switch off on their own. Wi-Fi stays on.
- **Lid closed never stops your agents**, in Lights Out or on the normal desktop.
- **Agent control:** `lightsout list | new | send | read | wait | notify`, plus `lightsout skill` for agents that drive other agents.

## Requirements

- 64-bit Intel or AMD PC with UEFI (most PCs from 2012 on). Windows 10/11 or a systemd Linux.
- TPM 2.0 if the drive is BitLocker-encrypted.
- Not yet: Snapdragon/ARM laptops, Macs. Some business and Surface laptops need "allow Microsoft UEFI CA" turned on in the BIOS; the installer checks this before changing anything.

## Safety

- Windows stays the default system. Lights Out starts only when you press the button (one-time boot), so a power cycle always returns to Windows.
- Nothing is repartitioned. The starter lives in the EFI partition, the system in `C:\LightsOut`.
- If Windows was hibernated, Lights Out opens your drive read-only.
- The drive key is sealed to this PC's TPM and to the exact Lights Out start chain.

## Build

GitHub Actions builds everything (Windows and Linux terminal, the small Linux, the signed starter) and tests the whole switch in a virtual PC with UEFI, TPM and an NTFS drive. See `.github/workflows/build.yml`.

## License

MIT. The Lights Out system image contains Debian packages under their own licenses. Agent CLIs are not included; you install the ones you use.

Lights Out is independent and not affiliated with Anthropic, OpenAI, Google or any agent vendor.
