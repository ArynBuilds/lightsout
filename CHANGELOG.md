# Changelog

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
