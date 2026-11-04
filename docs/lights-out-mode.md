# Lights Out mode

Press **Lights Out** and your computer keeps doing exactly one thing: running your agents. Press **Lights On** to come back. Nothing is lost on the way, in either direction.

## What happens on each system

### Windows

1. Lights Out saves your tabs, rooms, folders and what each tab runs.
2. Windows shuts down completely, as in a normal restart. Apps with unsaved work can still stop the shutdown and ask you first.
3. A small Linux starts in its place, from the starter Lights Out keeps next to Windows' own boot files. It unlocks your C: drive with its own key (no recovery key to type, see [Security](security.md)), joins your Wi-Fi with the networks Windows knew, and opens the same terminal with your tabs.
4. Agents continue their conversations: `claude` continues with `claude --continue`, `codex` with `codex resume --last`, and so on. Other commands are typed back in for you, without being run.

Your home folder in Lights Out is your Windows user folder, so your agents' logins, settings and projects are the same ones. Whatever you install in Lights Out stays installed for the next time.

**Lights On** restarts into Windows. The tabs you had in Lights Out (including new ones) come back in the Windows app.

### Linux

No restart. Lights Out stops the desktop (with its apps) and starts a full-screen session that shows only the Lights Out terminal, with your tabs. The power settings are applied; they are put back exactly as they were when you press **Lights On**, which brings your desktop back. Sign in and your tabs open again.

> **Note:** The desktop and its apps close. Save your work in other apps first.

### Mac

Lights Out quits your other apps politely (an app with unsaved work asks first), turns on Low Power Mode, keeps the Mac awake with the lid closed, and switches Spotlight indexing, Time Machine and Bluetooth off. Every other process runs at the lowest background priority, while Lights Out and your agents keep full speed. Local servers your agents may need, such as a database, are left at normal speed.

**Lights On** puts every setting back and reopens the apps that were quit.

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
