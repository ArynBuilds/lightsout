# Changelog

## 0.3.2-rc.8

A test build (`irm https://lightsout.sh/rc | iex`).

- **Everything goes back to normal, however Lights Out stops.**
  - Mac: a small background service puts your settings back (sleep, Low Power Mode, Spotlight, Time Machine, Bluetooth, slowed-down apps) when Lights Out stopped without Lights On: a crash, a force quit, a restart or an empty battery. It checks at start-up and every 30 seconds.
  - Linux PC: Bluetooth and the keyboard light are put back before a shutdown saves their state.
  - Lights Out mode on a Windows PC: a frozen system restarts by itself after a minute, into Windows.
  - A frozen screen: `Ctrl+Alt+Backspace` restarts the Lights Out screen (and on a Linux PC, brings the desktop back).
- **Battery time left** next to the battery % (and in `lightsout power`).
- **Hooks from Windows settings don't fail in Lights Out mode.** Your home folder is the Windows one, so CLI hooks and status lines that start a Windows program (a `C:\` path, an `.exe`, PowerShell) ran into "not found" on every event. They now quietly do nothing, since Windows is off.
- **The screen runs at its lowest refresh rate** (60 Hz instead of 240 Hz on fast laptop panels).

## 0.3.2-rc.7

A test build (`irm https://lightsout.sh/rc | iex`).

- **Touchpad scrolling works.** A touchpad moves a few pixels at a time, and each step was rounded down to zero lines. Now the steps add up.

## 0.3.2-rc.6

A test build (`irm https://lightsout.sh/rc | iex`).

- **Wi-Fi starts in Lights Out.** The system was missing `modprobe`, which Fedora's kernel uses to load extra pieces on demand. Without it the Wi-Fi service couldn't get a cipher it needs and quit, so agents couldn't be installed either.
- **`cd` takes Windows paths:** `cd c:/dev` or `cd 'C:\DEV\app'` go to `/mnt/c/DEV/app`.
- No more SELinux message at start (Lights Out doesn't use it).
- These fixes are inside the system file, which an update keeps while the kernel stays the same. To get them, delete `C:\LightsOut\system.img` before updating (programs installed inside Lights Out need installing again).

## 0.3.2-rc.5

A test build (`irm https://lightsout.sh/rc | iex`).

- **Lights Out opens a long-used Windows C:.** The Linux kernel is now Fedora's signed 7.2 (with Fedora's signed shim and boot loader) instead of Debian's 6.12. Before Linux 7.1, the NTFS driver couldn't open a drive whose file table had grown large and spread out, common on PCs used for a while, and failed with a bare "No such file or directory". Debian's 7.x kernels no longer include that driver; Fedora's do. Secure Boot stays on.
- Updating downloads the system file again once, because its kernel changed.

## 0.3.2-rc.4

A test build (`irm https://lightsout.sh/rc | iex`).

- **Your agents come back running:** reopening Lights Out (after a restart, or when the window or terminal was closed) continues each agent's conversation instead of only typing its command back in.
- **Each Claude tab gets its own chat back:** Lights Out remembers which chat each tab had and resumes exactly that one (`claude --resume`). Before, every Claude tab in the same folder reopened the newest chat.
- **A switch that didn't reach Lights Out:** back in Windows, the tabs saved at the switch come back.
- **Battery saver:** when Windows holds the switch back, Lights Out says so instead of waiting silently.
- **Read-only C: is caught:** if the drive can only be opened read-only, Lights Out stops and says why instead of starting without being able to save.
- **Updates keep the system file only while its Linux kernel matches** the new starter's. Otherwise the new system file is installed, so drivers such as Wi-Fi keep working.

## 0.3.2-rc.3

A test build (`irm https://lightsout.sh/rc | iex`).

- **Windows: the terminal view no longer closes by itself.** It closed after 5 seconds without a key press, while your agents kept running in the background.

## 0.3.2-rc.2

A test build (`irm https://lightsout.sh/rc | iex`).

- **Opening a BitLocker drive:** the starter opens the unlocked drive through its own device entry, and logs what it finds when that fails.

## 0.3.2-rc.1

A test build. Install it over your current Lights Out with `irm https://lightsout.sh/rc | iex` (Windows) or `curl -fsSL https://lightsout.sh/rc | sh` (Linux and Mac).

- **Opening a BitLocker drive:** if Linux doesn't create the drive's entry by itself, the starter now makes it. When the drive still can't be opened, the starter shows and saves the exact reason.
- **The switch works on battery:** Windows' default "only when plugged in" rule no longer holds it back.
- **Test builds:** `lightsout.sh/rc` installs the newest test build.

## 0.3.1

- **Faster updates:** re-running the installer no longer downloads the 634 MB system again; it keeps the one you have, so an update is about 40 MB.
- **Clipboard:** programs in a tab can still copy to the clipboard, but can no longer read it.
- **Drive key:** right after opening the drive, Lights Out locks the TPM chip until the next start, so nothing running later can take the BitLocker key out.

## 0.3.0

The first full release. Everything from 0.3.0-rc.1 and rc.2, plus:

- **Runs inside any terminal:** type `lightsout` in PowerShell, cmd, bash, zsh or any terminal app and it opens right there, like tmux. Shortcuts there are Ctrl+B then the key (F1 opens actions); `lightsout --window` opens the window.
- **Close the terminal, keep working:** your agents keep running in the background; `lightsout` brings you back to the same screen. Ctrl+B D detaches on purpose.
- **Rooms on a server:** `lightsout room add me@server` (or actions → New room on a server...) makes a room whose agents run on that server, 24/7. It shows live, reconnects by itself and lists what runs there. `lightsout open me@server` shows everything on a server.
- **Room commands:** `lightsout room add | list | remove`.
- **For agents:** a skill (`npx skills add ArynBuilds/lightsout`, or `lightsout skill`), and lightsout.sh/llms.txt with the docs for agents.
- **Many rooms, tabs and agents:** the rooms, the running list, the tab bar and the actions list scroll.
- **Actions:** nothing is chosen until you use the arrow keys; click any action to run it.
- **The mouse reaches programs** that use it (editors, `htop`); Shift keeps it for selecting.
- **Sidebar:** "rooms" and "running" headings.
- **Removed:** saved machines in the settings file (rooms on a server replace them).

## 0.3.0-rc.2

- **No more freezes:** saving your tabs no longer waits for Windows to list the running commands.
- **Quiet:** no system notifications on any system; alerts stay in the window, the sound is off unless you switch it on. An agent idling at its prompt never shows up as "done".
- **Clean tabs:** on Windows, tabs start from a fresh environment, so markers of whatever started Lights Out (another agent, a terminal) don't reach the agents inside.
- **No popping windows:** helper programs run without a window of their own.
- **Every character shows:** symbols, spinners and emoji missing from the built-in font come from the system's fonts, for any program.
- **Reopening** the app types each tab's command back in instead of starting agents; switching to and from Lights Out still continues them.
- **Rooms:** "Delete this room" in actions; the actions list sits in the middle of the screen.

## 0.3.0-rc.1

- **Mac: Lights Out logs you out.** Your agents run in a small background service meanwhile, so only macOS's core and your agents keep running; signing in brings the tabs back. It stays signed in when local servers would end with a log-out. `LIGHTSOUT_MAC_LOGOUT=0` at install keeps you signed in always.
- **Wi-Fi chooser** on Windows, Linux and Mac.
- **More cores only when needed:** no fixed watt cap; work starts on the most frugal cores and more join while work waits.
- **Safer switching:** no switch while a Windows update waits; tabs made in Lights Out come back to Windows; a read-only drive is announced.
- **Fixed:** `lightsout new -- ...` kept quotes in commands.

## 0.2.1

- **Lights Out mode for Windows, Linux and Mac.** Windows switches fully off and a small Linux runs only the terminal; Linux stops the desktop without a restart; Mac quits other apps and slows everything else down.
- **One-line install:** `irm https://lightsout.sh | iex` (Windows), `curl -fsSL https://lightsout.sh | sh` (Linux, Mac). `lightsout uninstall` removes everything.
- **No recovery key:** a BitLocker drive is opened with Lights Out's own key, sealed into the TPM.
- **The terminal:** rooms, tabs, splits, a sidebar with each agent's state, mouse, copy and paste, actions, alerts, your own keys, saved machines, live settings.
- **Agents continue** across switches (`resume.conf`), and `lightsout list | new | send | read | wait` lets agents run agents.
- **Power:** the extra graphics card powered off; Bluetooth, sound and keyboard light off; work on the most frugal cores, with more joining only while work waits.
- **Wi-Fi chooser** on every system.
- Closing the lid never stops your agents.

## 0.1.1

- Restores every setting on exit, runs as a single instance, and restores settings at sign-in after a crash.

## 0.1.0

- First release: a low-power mode for Windows with Windows still on (screen off, CPU capped, Bluetooth off, background apps throttled).
