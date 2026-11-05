#!/bin/bash
# Runs once inside the new system at build time.
set -e
chmod 755 /usr/local/sbin/* /usr/local/bin/* /usr/local/lib/lightsout/*   # every script of ours (a list misses new ones)

# The user. Its home folder is your own space: user.img on C:, kept by updates (see lightsout-home).
groupadd -f netdev
useradd -m -s /bin/bash -G sudo,video,input,render,netdev lights
passwd -d lights            # no password: the Lights Out lock screen protects the session
echo 'lights ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/lights
chmod 440 /etc/sudoers.d/lights

echo lightsout > /etc/hostname
printf '127.0.0.1 localhost\n127.0.1.1 lightsout\n' > /etc/hosts
sed -i 's/^# *en_US.UTF-8/en_US.UTF-8/' /etc/locale.gen && locale-gen >/dev/null
echo 'LANG=en_US.UTF-8' > /etc/default/locale

# Windows keeps the hardware clock in local time; read it that way, sync over the internet
printf '0.0 0 0.0
0
LOCAL
' > /etc/adjtime
systemctl enable systemd-timesyncd

# Wi-Fi: iwd handles DHCP itself; resolved does DNS
mkdir -p /etc/iwd
printf '[General]\nEnableNetworkConfiguration=true\n[Network]\nNameResolvingService=systemd\n' > /etc/iwd/main.conf
systemctl enable iwd systemd-resolved
ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf

systemctl enable lightsout-power.service lightsout-cores.service lightsout-wifi.service lightsout-session.service lightsout-selftest.service \
  lightsout-split.service lightsout-grow.timer lightsout-packages.service lightsout-started.service lightsout-learn.service
systemctl set-default graphical.target
# Nothing else on the screen: no login prompts
systemctl mask getty@tty1.service
# Lid, power button: Lights Out decides, not logind
mkdir -p /etc/systemd/logind.conf.d
printf '[Login]\nHandleLidSwitch=ignore\nHandleLidSwitchExternalPower=ignore\nHandlePowerKey=ignore\nIdleAction=ignore\n' \
  > /etc/systemd/logind.conf.d/lightsout.conf
# Fedora's kernel loads drivers on demand through /usr/bin/modprobe; Debian keeps it in /usr/sbin
ln -sf ../sbin/modprobe /usr/bin/modprobe
# driver settings this PC needs, learned once and kept on C: (see lightsout-learn)
ln -sf /mnt/c/LightsOut/config/modprobe.conf /etc/modprobe.d/lightsout.conf
# /bin/sh is dash behind a front that lets Windows programs (Windows is off) succeed quietly
dpkg-divert --local --no-rename --divert /usr/bin/sh.dash --add /usr/bin/sh
ln -sf /usr/local/lib/lightsout/sh /usr/bin/sh
# A frozen system restarts by itself after a minute, and a restart or shut-down that hangs
# is forced after 30 s (the chip's watchdog). On a Windows PC: into Windows, which checks the drive
mkdir -p /etc/systemd/system.conf.d
printf '[Manager]\nRuntimeWatchdogSec=1min\nRebootWatchdogSec=30s\n' > /etc/systemd/system.conf.d/lightsout.conf
# Start and stop show the PC's own logo with a spinner, no text (messages go to the log).
# The last frame stays until the terminal draws; at a restart the splash takes the screen
# once the terminal is gone.
printf '[Daemon]\nTheme=bgrt\n' > /etc/plymouth/plymouthd.conf
mkdir -p /etc/systemd/system/plymouth-quit.service.d
printf '[Unit]\n# the splash stays until just before the terminal starts (after your space and Wi-Fi are ready)\nAfter=lightsout-wifi.service lightsout-split.service\n[Service]\nExecStart=\nExecStart=-/usr/bin/plymouth quit --retain-splash\n' > /etc/systemd/system/plymouth-quit.service.d/lightsout.conf
for u in reboot poweroff halt; do
  mkdir -p "/etc/systemd/system/plymouth-$u.service.d"
  printf '[Unit]\nAfter=lightsout-session.service\n' > "/etc/systemd/system/plymouth-$u.service.d/lightsout.conf"
done
# Smaller journal, kept in RAM (fewer disk writes = less power)
mkdir -p /etc/systemd/journald.conf.d
printf '[Journal]\nStorage=volatile\nRuntimeMaxUse=32M\n' > /etc/systemd/journald.conf.d/lightsout.conf

# Agent configs follow you from Windows (see lightsout-env.sh)
echo '. /usr/local/lib/lightsout/lightsout-env.sh' >> /home/lights/.bashrc

apt-get clean
rm -rf /var/lib/apt/lists/* /usr/share/doc/* /usr/share/man/?? /usr/share/man/??_*
