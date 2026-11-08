# Changelog

## 0.3.2-rc.18

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **Nothing you set up is lost any more:** a room you rename or add, a tab you open or close, a folder or a program that changes is saved within 2 seconds. Before, some of these waited for the next agent activity or for Quit, so a crash, an update or a forced close could bring back an older set of rooms and tabs.

## 0.3.2-rc.17

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **Lighter on battery, on Windows and Mac:** to know what runs in each tab (so agents come back after a switch or a restart), the terminal used to start a helper program: PowerShell on Windows each time a program started or ended in a tab (about 0.7 s of CPU each time), and `ps` on the Mac every 1.5 s while agents worked, plus `pmset` every 20 s for the battery. It now asks Windows and macOS directly, which costs next to nothing (Linux already did). Windows, Linux and Mac share the same code for it.
- **Lighter in Lights Out mode:** the helper that adds cores while agents are busy checks every 5 seconds; each check used to start four small programs, each waking the processor. It now reads what it needs itself.

## 0.3.2-rc.16

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **An empty battery no longer cuts Lights Out off.** At 10% a warning shows (and "plug in!" next to the battery); at 2% Lights Out saves your tabs and shuts down cleanly, closing and locking the drive first. The next start is Windows (or your Linux desktop), and the tabs come back there. It also works with the screen off.

## 0.3.2-rc.15

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **The restart record starts at Lights On** and marks each stage (programs stopped, systemd's part done, the last steps), with the kernel's last messages at each, so a restart that stops anywhere tells where and why.
- Tabs that come back with an agent (Claude, Codex...) no longer show the old screen text under the agent's own.

## 0.3.2-rc.14

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **Your tabs come back after a crash or Ctrl+Alt+Backspace in Lights Out.** The terminal saves them there too, right after a program starts or ends in a tab, so agents continue their conversation.
- **Agents you start by typing (`claude`, `codex`...) come along on Lights On** on Linux and Mac, as they did on Windows: the terminal now sees what runs in each tab there too, npm programs included.
- **No more "volume was not properly unmounted" message.** A forced power-off leaves the EFI partition marked, and Linux kept the mark for good. The next start checks the partition, which clears it.
- **Restart into Windows:** on some laptops the last restart step waited until a key or the power button was pressed. The cores now stay awake for that step (the restart record says so).
- **Battery:** on battery, about 4 screen updates a second while you aren't typing, clicking or scrolling (full speed for a second after you do).
- **Brightness-down key** works again.

## 0.3.2-rc.13

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **Right from the first start:** what a PC needs is set up before you ever see the Lights Out terminal.
  - PCIe link power saving: the installer reads from the BIOS (the same table Linux reads) whether it keeps that from Linux, and tries the setting from the very first start. It stays only if that start comes up.
  - Brightness on OLED panels that need another control: noticed during the first start, before the terminal shows. Lights Out then starts once more, straight into Lights Out (the splash stays), and the setting is kept on C:, so updates don't need to learn it again.
- Power settings no longer take cores back from the helper that adds them while work is waiting.

## 0.3.2-rc.12

A test build (`lightsout update`, or `irm https://lightsout.sh/rc | iex`).

- **Lights On restarts into Windows on BitLocker PCs.** The restart hung at its last step, locking the drive again: the tool waited for an answer from a part of Linux that was already stopped. It no longer waits for it, and never more than 10 seconds. (Found with the new restart record.)

## 0.3.2-rc.11

A test build (`irm https://lightsout.sh/rc | iex`, or `lightsout update`).

- **A restart that hangs ends in Windows by itself.** Linux used to switch the PC's hardware watchdog off at the start of a restart, exactly where it can hang; it now stays on, so about 30 seconds after any hang the PC starts Windows.
- **Every start and restart leaves a short record**, copied to `C:\LightsOut\logs\boot` at each Windows start (no admin rights needed to read it), so a problem on any PC can be read afterwards.
- **Settings tried once, kept only if they work:** Lights Out can try a kernel setting a PC seems to need (first: PCIe link power saving, which some BIOSes keep from Linux). If the start with it comes up, it stays; if not, the next start takes it off and says so in one line.
- **Battery:** the terminal redraws at most 10 times a second on battery, and only the part of the screen that changed goes to the screen. `lightsout-cores`, which keeps agents on the frugal cores, now actually runs.
- **Brightness:** the brightness keys work in Lights Out (you're allowed to change it now), and OLED panels that take brightness over the display link get that control.
- **The start-up splash stays until the terminal is there** (no black gap).
- **Test builds:** `lightsout.sh/rc` finds the newest test build itself, so installing no longer stops at GitHub's hourly limit.
- Fixes: updates back up the previous starter too; `uninstall -KeepSystem` removes the rest again; a new system file has room enough not to grow at its first start.

## 0.3.2-rc.10

A test build (`irm https://lightsout.sh/rc | iex`).

- **`lightsout update`** on Windows, Linux, Mac and inside Lights Out mode: the newest version from the channel you installed, and everything you have stays. See [Update](docs/update.md).
- **Your own space in Lights Out mode:** what you install inside Lights Out (agents like Claude Code, `~/.local`, nvm, cargo), logins and SSH keys now live in `C:\LightsOut\user.img`, apart from the system file, so updates keep them. Existing installs move there by themselves at the next start.
- **Packages you install with `apt` come back** by themselves after an update.
- **A safe update:** the previous version is kept until the new one has started; if it doesn't, Windows puts it back.
- **Growing the Lights Out files works**: the tool it needed was missing from the system.
- **Security:** the scripts Windows runs as SYSTEM (`C:\LightsOut\bin`) can only be changed by admins.

## 0.3.2-rc.9

A test build (`irm https://lightsout.sh/rc | iex`).

- **No more text while switching.** Going into Lights Out and back shows your PC's own logo with a spinner, like a normal start. If something stops the start, one line under the spinner says why; the details go to the log.
- **Lights On restarts into Windows without hanging.** Some laptops hung at the last step of the restart (the firmware's own restart call, which Lights Out used). It now restarts the usual way, and if a restart or shut-down still hangs, the chip's watchdog forces it after 30 seconds.
- **Back in Windows, Lights Out opens by itself** with your tabs (only when you come back from Lights Out; otherwise it stays closed at sign-in).
- **`lightsout close PANE`**: agents can close a pane, the same as "Close pane" in the actions.
- **Everything goes back to normal, however Lights Out stops.**
  - Mac: a small background service puts your settings back (sleep, Low Power Mode, Spotlight, Time Machine, Bluetooth, slowed-down apps) when Lights Out stopped without Lights On: a crash, a force quit, a restart or an empty battery. It checks at start-up and every 30 seconds.
  - Linux PC: Bluetooth and the keyboard light are put back before a shutdown saves their state.
  - Lights Out mode on a Windows PC: a crashed terminal comes back within seconds with your tabs (before, the screen stayed black), and a frozen system restarts by itself after a minute, into Windows.
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
