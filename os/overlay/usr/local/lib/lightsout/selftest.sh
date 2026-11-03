#!/bin/bash
# Prints markers to the serial console for the build test, then shuts down.
s() { echo "LIGHTSOUT-SELFTEST: $*" > /dev/ttyS0; }
sleep 20
s "booted kernel=$(uname -r)"
s "starter: $(grep -E 'sealed|unsealed' /run/lightsout/starter.log 2>/dev/null | tr '\n' ' ')"
s "root=$(findmnt -no SOURCE /) c=$(findmnt -no FSTYPE,OPTIONS /mnt/c | cut -c1-40)"
if touch /mnt/c/LightsOut/selftest-write 2>/dev/null; then s "c-drive-write=ok"; else s "c-drive-write=FAILED"; fi
s "session=$(systemctl is-active lightsout-session.service) wifi-import=$(systemctl is-active lightsout-wifi.service)"
if pgrep -x lightsout-app >/dev/null; then s "terminal=running"; elif pgrep -x foot >/dev/null; then s "terminal=foot-fallback"; else s "terminal=NOT-RUNNING"; fi
if mountpoint -q /mnt/c/Users/tester/proj/node_modules && [ ! -e /mnt/c/Users/tester/proj/node_modules/win-only ]; then s "split=ok"; else s "split=FAILED"; fi
s "home=$(getent passwd lights | cut -d: -f6) per-system-local=$(mountpoint -q /mnt/c/Users/tester/.local && echo yes || echo no)"
s "resume-claude=$(grep -E '^claude' /usr/local/share/lightsout/resume.conf | tr -s ' ')"
api=$(runuser -u lights -- env XDG_RUNTIME_DIR=/run/user/1000 /usr/local/bin/lightsout list 2>&1 | tr '
' ' ' | cut -c1-160)
s "api: $api"
s "windows-user=$(cat /etc/lightsout/windows-user 2>/dev/null)"
s "iwd-networks=$(ls /var/lib/iwd 2>/dev/null | tr '\n' ' ')"
s "failed-units=$(systemctl --failed --no-legend --plain | awk '{print $1}' | tr '\n' ' ')"
for u in $(systemctl --failed --no-legend --plain | awk '{print $1}'); do
  journalctl -b -u "$u" -n 8 --no-pager -o cat 2>/dev/null | sed "s/^/LIGHTSOUT-SELFTEST: [$u] /" > /dev/ttyS0
done
journalctl -b -u lightsout-session -n 25 --no-pager -o cat 2>/dev/null | sed 's/^/LIGHTSOUT-SELFTEST: [session] /' > /dev/ttyS0
s "mounts: $(findmnt -no OPTIONS /mnt/c | grep -o nocase || echo case-sensitive)"
s "done, powering off"
systemctl poweroff
