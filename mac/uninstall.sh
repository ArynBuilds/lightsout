#!/bin/sh
# Removes Lights Out from this Mac (run with sudo: `lightsout uninstall`).
/usr/local/lib/lightsout/switch in 2>/dev/null
/usr/local/lib/lightsout/switch sleep 2>/dev/null
ME="${SUDO_USER:-$(id -un)}"
home=$(dscl . -read "/Users/$ME" NFSHomeDirectory 2>/dev/null | awk '{print $2}')
rm -f /etc/sudoers.d/lightsout /usr/local/bin/lightsout /usr/local/bin/lightsout-app
rm -rf /usr/local/lib/lightsout /var/run/lightsout "/Applications/Lights Out.app"
[ -n "$home" ] && rm -rf "$home/.local/state/lightsout"
echo "Lights Out removed."
