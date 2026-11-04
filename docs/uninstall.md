# Uninstall

```sh
lightsout uninstall
```

It asks for admin rights and removes everything Lights Out added.

**Windows**

- the boot entry and the starter in the EFI partition
- Lights Out's own BitLocker key (your other keys are untouched)
- `C:\LightsOut`, with the Lights Out system and everything installed inside it
- the scheduled tasks, the Start menu entry and the `lightsout` command
- the app's saved tabs and settings

To keep what you installed in Lights Out for a later install, run the uninstaller with `-KeepSystem`:

```powershell
powershell -ExecutionPolicy Bypass -File C:\LightsOut\bin\uninstall.ps1 -KeepSystem
```

**Linux**

- the `lightsout` commands, Lights Out mode (its systemd units and its sudo rule)
- the app's menu entry and saved tabs

**Mac**

- the `lightsout` commands, the helper and its sudo rule, **Lights Out** in Applications
- every setting Lights Out changed is put back first

Your agents, their logins and your files are never touched.
