# Security

Lights Out changes how your computer starts, and on Windows it opens your encrypted drive. This page explains exactly how, and what it can and cannot reach.

## Starting Lights Out on Windows

- **Windows stays your default system.** Lights Out adds one entry to the computer's boot list, after Windows. When you press Lights Out, it asks the firmware to start that entry **once**, for the next start only. Any later start, including after a power cut, is Windows again.
- **Secure Boot stays on.** The starter is Debian's Microsoft-signed `shim`, then Debian's signed boot loader and signed Linux kernel. Nothing is unsigned and nothing needs Secure Boot switched off.
- **Nothing is repartitioned.** The starter (about 45 MB) sits in the EFI partition next to Windows' boot files. The system itself is one file, `C:\LightsOut\system.img`, on your encrypted drive.

## Opening a BitLocker drive without a recovery key

1. At install, Lights Out adds a **key of its own** to BitLocker, next to your existing ones. Your recovery key is never read, shown or stored.
2. When you press Lights Out, a task running as Windows' system account hands that key to the starter for that one start, on the boot partition.
3. The starter **seals** the key into the PC's TPM security chip and deletes the file at once. Sealed, the key can only be opened by the same chip, and only when the PC starts with the exact same firmware, Secure Boot settings and Lights Out boot chain.
4. On every start, the starter unseals the key, opens C:, and forgets the key once the drive is open.
5. Right after that, it changes one of the measured values in the TPM. Until the next start, the chip won't release the key again, not even to a program running as administrator.

A task that runs at every Windows start removes any key or one-time pass left on the boot partition, for example if a switch was interrupted. Removing Lights Out removes its BitLocker key too.

If the key can't be unsealed, for example after a firmware update, Lights Out never asks for your recovery key: it starts Windows again, and the next switch from Windows hands the key over anew.

## The lock screen

A start that came from the Lights Out button in Windows carries a one-time pass and opens straight to your tabs. Any other start, for example one chosen from the boot menu, shows the lock screen and asks for your PIN. The PIN is stored only as a salted hash, on your encrypted drive.

## What Lights Out can reach

- **Your files:** the Lights Out system opens C: with your Windows user's rights, as your agents would on Windows. Agents run as your user, never as an administrator.
- **Hibernated Windows:** if Windows wasn't fully shut down, C: is opened read-only so nothing can be damaged.
- **Logins:** your agents use the logins they already have in your home folder. Lights Out doesn't copy, upload or read them.
- **Clipboard:** programs in a tab can copy to your clipboard but can't read it, so a file you open or a server you log into can't take a copied password.
- **Network:** the control commands listen only on `127.0.0.1`, behind a random token that only your user can read. Nothing listens on your network.

## The installers

`irm https://lightsout.sh | iex` and `curl -fsSL https://lightsout.sh | sh` run the installers from this repository, [`installer/install.ps1`](https://github.com/ArynBuilds/lightsout/blob/main/installer/install.ps1) and [`installer/install.sh`](https://github.com/ArynBuilds/lightsout/blob/main/installer/install.sh). They download only from the repository's GitHub Releases, which are built in the open by GitHub Actions, and the system image is checked against its SHA-256 checksum.

## Reporting a vulnerability

Please don't open a public issue. See [SECURITY.md](https://github.com/ArynBuilds/lightsout/blob/main/SECURITY.md).
