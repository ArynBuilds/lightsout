# Changelog

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
