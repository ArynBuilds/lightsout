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
systemctl is-system-running --wait >/dev/null 2>&1   # every service has started (or failed) by now
s "failed-units=$(systemctl --failed --no-legend --plain | awk '{print $1}' | tr '\n' ' ')"
for u in $(systemctl --failed --no-legend --plain | awk '{print $1}'); do
  journalctl -b -u "$u" -n 8 --no-pager -o cat 2>/dev/null | sed "s/^/LIGHTSOUT-SELFTEST: [$u] /" > /dev/ttyS0
done
journalctl -b -u lightsout-session -n 25 --no-pager -o cat 2>/dev/null | sed 's/^/LIGHTSOUT-SELFTEST: [session] /' > /dev/ttyS0
s "cd-windows-path=$(bash -c '. /usr/local/lib/lightsout/lightsout-env.sh; cd c:/users && pwd')"
s "windows-commands=$(sh -c '"C:/x/hook.exe" Stop'; echo $?) $(sh -c 'powershell -File C:\\x.ps1'; echo $?) $(sh -c 'echo ok.exe') $(sh -c 'exit 3'; echo $?)"
s "mounts: $(findmnt -no OPTIONS /mnt/c | grep -o nocase || echo case-sensitive)"
s "splash=$(systemctl show -p Result --value plymouth-start.service)/$(sed -n 's/^Theme=//p' /etc/plymouth/plymouthd.conf)"
# your own space and your packages survive an update (the test puts a fresh system file in
# place between boot 1 and boot 2)
if [ -f /home/lights/.update-test ]; then
  if grep -qxF "$(cat /home/lights/.update-test)" /home/lights/.lightsout-packages; then kept=yes; else kept=packages-lost; fi
else
  p=$(apt-mark showauto | head -n 1); apt-mark manual "$p" >/dev/null
  /usr/local/lib/lightsout/packages save && /usr/local/lib/lightsout/packages restore && echo "$p" > /home/lights/.update-test
  kept=new
fi
s "own-space=$(losetup -nO BACK-FILE "$(findmnt -no SOURCE /home/lights)" 2>/dev/null) kept=$kept"
# kernel options on trial (lightsout-tries): boot 2 puts one on trial (boot 1 keeps its boot
# line, so boot 2 tests unsealing), boot 3 starts with it (kept) and puts one on trial that
# boot 4, a start that never comes up, uses (boot 5: off)
T=/boot/efi/EFI/lightsout/tries
if [ "$kept" != new ]; then
  if grep -q '^lightsout.works=1 ' "$T" 2>/dev/null; then
    grep -q '^lightsout.fails=1 ' "$T" || echo "lightsout.fails=1 try" >> "$T"
  else
    echo "lightsout.works=1 try" >> "$T"
  fi
  /usr/local/sbin/lightsout-tries
fi
s "tries: $(tr '\n' ' ' < "$T")on-this-start=$(grep -o 'lightsout.works=1' /proc/cmdline)"
s "watchdog=$(systemctl show -p RuntimeWatchdogUSec --value) at-restart=$(systemctl show -p RebootWatchdogUSec --value) kept-on-in-restart=$(cat /sys/module/watchdog/parameters/stop_on_reboot 2>/dev/null)"
# a crashed or frozen terminal (the emergency key Ctrl+Alt+Backspace kills it): a new one must come up
old=$(pgrep -xn lightsout-app)
key='bindsym --locked Ctrl+Alt+BackSpace exec pkill -KILL -x lightsout-app'
if runuser -u lights -- env XDG_RUNTIME_DIR=/run/user/1000 SWAYSOCK="$(ls /run/user/1000/sway-ipc.*.sock 2>/dev/null | head -n1)" \
     swaymsg -t get_config 2>/dev/null | grep -qF "$key" && pkill -KILL -x lightsout-app; then
  for i in $(seq 30); do new=$(pgrep -xn lightsout-app); [ -n "$new" ] && [ "$new" != "$old" ] && break; sleep 1; done
fi
if [ -n "${new:-}" ] && [ "$new" != "$old" ]; then s "screen-restart=ok"; else s "screen-restart=FAILED"; fi
s "done, restarting (as Lights On does; the test PC stops there)"
systemctl reboot
