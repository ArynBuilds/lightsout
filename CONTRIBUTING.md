# Contributing

Thanks for helping. Bug reports, hardware reports and pull requests are all welcome.

## Reporting a bug

Open an [issue](https://github.com/ArynBuilds/lightsout/issues/new/choose) with your system (Windows, Linux or Mac, and its version), your laptop model, what you did, what you expected and what happened. For install problems on Windows, attach `C:\LightsOut\logs\install.log`.

**Hardware reports help a lot:** does Lights Out mode work on your laptop, does Wi-Fi work, and what power draw do you see (`lightsout power` with the screen off)?

## The code

| Folder | What |
|---|---|
| `term/` | The terminal app and the `lightsout` command (Rust): Windows, Linux, Mac |
| `os/` | The small Linux that runs when Windows is off (Debian, built with `mmdebstrap`) |
| `starter/` | The signed starter in the EFI partition: unlocks C: and starts the system |
| `linux/` | Lights Out mode on a Linux PC (systemd units, the switch helper) |
| `mac/` | Lights Out on a Mac (the switch helper) |
| `installer/` | The one-line installers and the Windows uninstaller |
| `test/` | The virtual-PC test: UEFI, TPM and an NTFS drive, two starts |
| `lightsoutwebsite/` | lightsout.sh, built with Forjr |
| `docs/` | The documentation |

## Building

The terminal:

```sh
cd term
cargo build --release
cargo clippy --release -- -D warnings
cargo test --release
```

Everything else (the system image, the starter, the virtual-PC test, the Linux and Mac checks) is built and tested by GitHub Actions on every push: see `.github/workflows/build.yml`. A tag `vX.Y.Z` publishes a release.

## Pull requests

- Keep a change focused on one thing, and say why in the description.
- `cargo clippy -- -D warnings` and `cargo test` pass.
- Nothing system-specific is hard-coded: programs, agents and folders come from data files (`resume.conf`, `per-system.conf`), never from lists in the code.
- Power settings are general: a knob a machine lacks is skipped, never assumed.
- Anything that touches boot, BitLocker or the TPM needs the virtual-PC test to pass, and a note on how you tested it.
