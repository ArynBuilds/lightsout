# Troubleshooting

## Installing

**"this PC does not start with UEFI"**
The PC starts in the old BIOS way. Lights Out on Windows needs UEFI, which most PCs from 2012 on use; the installer stops before changing anything.

**"C: is encrypted (BitLocker) but this PC has no ready TPM security chip"**
Turn the TPM on in the firmware settings (often called TPM, PTT on Intel, or fTPM on AMD), then run the installer again.

**"not enough room on the EFI partition"**
The boot partition is almost full. Lights Out needs about 45 MB there. A slimmer starter for small partitions is planned.

**Windows says the app is unknown or blocks it**
On PCs with Smart App Control switched on, Windows may block programs that aren't signed yet. Code signing is coming; until then, Lights Out needs Smart App Control off.

## Switching

**"Windows has an update waiting"**
Restart Windows once to let it finish the update, then press Lights Out again.

**The PC started Windows instead of Lights Out**
Lights Out only starts when it was asked for, once. If Windows started instead, the firmware didn't take the one-time request, or the starter couldn't open the drive; your tabs are still in the Windows app. Press Lights Out again. If it keeps happening, the firmware may block other systems: some business and Surface laptops need **"Allow Microsoft 3rd-party UEFI CA"** switched on in the firmware settings.

**"C: is read-only"**
Windows was hibernated or not shut down fully (Fast Startup counts). Go back with Lights On, shut Windows down fully or restart it, then switch again.

**No Wi-Fi in Lights Out**
Open **actions → Wi-Fi...** and pick your network. A few very new Wi-Fi chips aren't supported by Linux yet.

**Linux: the desktop doesn't come back**
Lights On should bring it back. If the screen stays on a text console, sign in there and run `sudo systemctl isolate default.target`.

**Mac: Lights Out didn't log me out**
Local servers were running (anything listening on a port, such as a database or a dev server), and logging out would have ended them. Lights Out stayed signed in and slowed everything else down instead. Stop the servers you don't need and press Lights Out again.

## Agents

**An agent's command was typed back in instead of continuing its conversation**
Lights Out continues an agent only when it knows that tab's own conversation; it never guesses. Agents that aren't in `resume.conf` are typed back in: add yours, see [Restoring your tabs](session-state.md#adding-an-agent). For Codex, approve Lights Out's hook once (`/hooks` in Codex). Right after updating from an older version, the first reopen may still miss a conversation, because the older version saved the tabs.

**A plugin wasn't installed: "... is already used by ..."**
Its key belongs to your settings, another plugin or Lights Out. The message says how to fix it: see [Plugins](plugins.md#keys).

**A plugin does nothing**
Its log is in `plugin-data/ID/log.txt` (see [Plugins](plugins.md#where-things-are)). `lightsout plugin list` shows whether it is on, and `broken` when its `lightsout-plugin.toml` has a mistake.

**Ctrl+click on a link says "No browser here"**
In Lights Out mode on a Windows PC there is no desktop, so no browser. A [plugin](plugins.md#links) can handle links there.

**A program shows as working but waits for me**
Add the phrase it asks with to `needs_you`: see [Configuration](configuration.md#when-an-agent-asks-in-its-own-words).

**`lightsout list` says it can't reach the app**
Open the app first (`lightsout`). With `--session NAME`, use the same `--session NAME` for the commands.

## Getting help

Open an issue at [github.com/ArynBuilds/lightsout/issues](https://github.com/ArynBuilds/lightsout/issues) with your system, what you did and what happened. On Windows, `C:\LightsOut\logs\install.log` helps with install problems.
