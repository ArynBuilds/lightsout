# Security policy

Lights Out changes how your computer starts and, on Windows, opens your encrypted drive. Security reports are taken seriously and answered quickly.

## Reporting a vulnerability

Please **don't open a public issue.** Report it privately through [GitHub's private vulnerability reporting](https://github.com/ArynBuilds/lightsout/security/advisories/new) for this repository.

Include what you found, how to reproduce it, and what an attacker could do with it. You'll get an answer within a few days, and credit in the release notes if you want it.

## Supported versions

Only the latest release gets security fixes.

## How it works

[docs/security.md](docs/security.md) explains how the drive key is sealed into the TPM, how the boot chain stays signed, and what Lights Out can reach.
