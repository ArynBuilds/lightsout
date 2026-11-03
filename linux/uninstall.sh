#!/bin/sh
# Removes Lights Out from this Linux PC (run with sudo: `lightsout uninstall`).
ME="${SUDO_USER:-$(id -un)}"
home=$(getent passwd "$ME" | cut -d: -f6)
systemctl isolate default.target 2>/dev/null
for u in lightsout.target lightsout-power-inplace.service lightsout-kiosk.service lightsout-return.service; do
  rm -f "/etc/systemd/system/$u"
done
systemctl daemon-reload
rm -f /etc/sudoers.d/lightsout /usr/local/bin/lightsout /usr/local/bin/lightsout-app /usr/local/sbin/lightsout-power
rm -rf /usr/local/lib/lightsout /run/lightsout
rm -f "$home/.local/share/applications/lightsout.desktop" "$home/.config/autostart/lightsout-return.desktop"
rm -rf "$home/.local/state/lightsout"
echo "Lights Out removed."
