# Update

```sh
lightsout update
```

It installs the newest version from the channel you installed: the latest release, or test builds if you installed with `lightsout.sh/rc`. Everything you have stays: settings, tabs, logins, and the programs and agents you installed inside Lights Out.

- **Windows:** the installer runs again and asks for admin rights. An open Lights Out window closes and opens again with your tabs; your agents continue.
- **In Lights Out mode:** Lights Out restarts into Windows, Windows installs the update before anyone signs in, then comes straight back to Lights Out with your tabs.
- **Linux and Mac:** the installer runs again.

## What keeps your things (Lights Out mode on a Windows PC)

- **Your own space**, `C:\LightsOut\user.img`, holds what you install inside Lights Out (agents like Claude Code, `~/.local`, nvm, cargo and the like), their logins and your SSH keys. Updates never replace it. It grows when it fills up, while Windows keeps at least 6 GB free.
- **The system file**, `C:\LightsOut\system.img`, is replaced only when the new version changed it. Packages you installed with `apt` are installed again by themselves the first time Lights Out is online.
- **The previous version** is kept until the new one has started. If it doesn't start, Windows puts the previous version back at its next start.
