# Lights Out mode

Press **Lights Out** and your computer keeps doing exactly one thing: running your agents. Press **Lights On** to come back. Nothing is lost on the way, in either direction.

## What happens on each system

### Windows

1. Lights Out saves your tabs, rooms, folders and what each tab runs.
2. Windows shuts down completely, as in a normal restart. Apps with unsaved work can still stop the shutdown and ask you first.
3. A small Linux starts in its place, from the starter Lights Out keeps next to Windows' own boot files. It unlocks your C: drive with its own key (no recovery key to type, see [Security](security.md)), joins your Wi-Fi with the networks Windows knew, and opens the same terminal with your tabs.
4. Agents continue their conversations: each `claude` tab its own chat, `codex` with `codex resume --last`, and so on. Other commands are typed back in for you, without being run.

Your home folder in Lights Out is your Windows user folder, so your agents' logins, settings and projects are the same ones. Whatever you install in Lights Out stays installed for the next time.

**Lights On** restarts into Windows. The tabs you had in Lights Out (including new ones) come back in the Windows app.

### Linux

No restart. Lights Out stops the desktop (with its apps) and starts a full-screen session that shows only the Lights Out terminal, with your tabs. The power settings are applied; they are put back exactly as they were when you press **Lights On**, which brings your desktop back. Sign in and your tabs open again.

> **Note:** The desktop and its apps close. Save your work in other apps first.

### Mac

1. Lights Out saves your tabs and quits your other apps politely: an app with unsaved work asks first.
2. Your agents move into a small background service that keeps running without anyone signed in. Known agents continue their conversations there.
3. You're logged out. Every app and helper of yours ends; only macOS's core, Wi-Fi and your agents keep running. Low Power Mode is on, the Mac stays awake with the lid closed, and Spotlight indexing, Time Machine and Bluetooth are off.

**Coming back:** sign in. Lights Out opens by itself, takes your tabs back from the background service, puts every setting back and reopens the apps that were quit.

> **Note:** If local servers are running that your agents may need, such as a database, logging out would end them. Lights Out then stays signed in instead: nothing is stopped, and everything except your agents and those servers runs at the lowest background priority.

To always stay signed in, install with `LIGHTSOUT_MAC_LOGOUT=0`:

```sh
curl -fsSL https://lightsout.sh | LIGHTSOUT_MAC_LOGOUT=0 sh
```

## The screen

| Do | How |
|---|---|
| Screen off | **Ctrl+Shift+O**, or close the lid |
| Screen on | Any key, a click or tap, or open the lid |

Closing the lid never stops your agents in Lights Out mode.

## The lock screen (Windows)

When Lights Out starts because you pressed the button in Windows, it opens straight to your tabs. If it ever starts any other way, for example chosen from the boot menu, it shows a lock screen first and asks for the PIN you chose at install.

## Things to know

- **A Windows update is waiting.** Lights Out won't switch until Windows has finished it: restart Windows once, then press Lights Out again. Otherwise the update would be interrupted.
- **Windows was hibernated.** If Windows wasn't shut down fully, Lights Out opens C: read-only to protect your files, and tells you. Shut Windows down or restart it before switching.
- **Windows programs** (`.exe` files) don't run in Lights Out. Projects with native parts, such as `node_modules`, get their own Linux copy automatically; your Windows copy stays untouched.
- **Wi-Fi:** the networks Windows knows are imported. To join another one, use **actions → Wi-Fi...**.
