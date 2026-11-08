#!/usr/bin/env bash
# Boots the real starter + system image in a virtual PC (UEFI with Secure Boot, virtual TPM)
# with a fake Windows "C:" (NTFS) and checks the whole chain, twice:
#   boot 1: one-time key handoff gets sealed into the TPM, system boots, writes to C:
#   boot 2: after an update (a fresh system file), the key is unsealed from the TPM, the
#           system boots again and your own space (user.img) and packages are still there
#   boot 3: with a kernel option on trial, put there as the installer does (a new boot line,
#           so the key is handed over again, as from Windows): it comes up, so it is kept
#   boot 4: the partition is cut 64 MB shorter than the file system says (as on a PC where
#           the partition shrank), so the driver can only open C: read-only: it must stop
#   boot 5: boot 4 used another option on trial and never came up: it is taken off again
#   after:  the NTFS "C:" must be cleanly closed (Windows would not complain)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
A="$here/.."; w="${VMTEST_DIR:-$here/work}"; rm -rf "$w"; mkdir -p "$w"/{tpm,mnt}
STARTER="$A/starter/out"; SYSIMG="$A/os/out/system.img"

# ---- ESP (FAT): starter + firmware fallback path + test switches
truncate -s 200M "$w/esp.img"; mkfs.vfat -F 32 -n EFI "$w/esp.img" >/dev/null
mmd -i "$w/esp.img" ::/EFI ::/EFI/BOOT ::/EFI/lightsout ::/EFI/fedora
mcopy -i "$w/esp.img" "$STARTER"/EFI/lightsout/* ::/EFI/lightsout/
mcopy -i "$w/esp.img" "$STARTER/EFI/lightsout/shimx64.efi" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$w/esp.img" "$STARTER/EFI/lightsout/grubx64.efi" ::/EFI/BOOT/grubx64.efi
sed 's#^\(linux .*\)$#\1 console=ttyS0,115200 ignore_loglevel printk.devkmsg=on#' "$STARTER/EFI/lightsout/grub.cfg" > "$w/grub.cfg"   # the screen and every message go to the test log
mcopy -o -i "$w/esp.img" "$w/grub.cfg" ::/EFI/fedora/grub.cfg
mcopy -o -i "$w/esp.img" "$w/grub.cfg" ::/EFI/lightsout/grub.cfg
printf 'TEST-1234-5678-ABCD\n' > "$w/handoff.key"; mcopy -i "$w/esp.img" "$w/handoff.key" ::/EFI/lightsout/handoff.key
: > "$w/selftest"; mcopy -i "$w/esp.img" "$w/selftest" ::/EFI/lightsout/selftest
mcopy -i "$w/esp.img" "$w/selftest" ::/EFI/lightsout/created-fedora-dir   # as the installer leaves it

# ---- fake C: (NTFS) holding LightsOut\system.img, a Wi-Fi profile and the user name
sys_mb=$(( $(stat -c %s "$SYSIMG") / 1048576 ))
# (room for your own space, user.img: the first start makes it only while Windows keeps 6 GB)
truncate -s "$(( sys_mb + 12000 ))M" "$w/c.img"; mkntfs -q -F -f -L Windows "$w/c.img"
ntfs-3g "$w/c.img" "$w/mnt" 2>/dev/null || mount -t ntfs3 -o loop "$w/c.img" "$w/mnt"
mkdir -p "$w/mnt/LightsOut/wifi" "$w/mnt/LightsOut/config" "$w/mnt/Users/tester/.claude"
echo "=== system file before boot 1:"; e2fsck -fn "$SYSIMG" 2>&1 | tail -n 2
# written in full like the Windows installer does (sparse NTFS files are a known weak spot)
cp --sparse=never "$SYSIMG" "$w/mnt/LightsOut/system.img"
printf 'tester' > "$w/mnt/LightsOut/config/windows-user"
printf "us" > "$w/mnt/LightsOut/config/keyboard"
mkdir -p "$w/mnt/Users/tester/proj/node_modules/win-only"
printf '{"name":"proj"}' > "$w/mnt/Users/tester/proj/package.json"
cat > "$w/mnt/LightsOut/wifi/test.xml" <<'EOF'
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1"><name>Güler iPhone’u</name>
<SSIDConfig><SSID><name>Güler iPhone’u</name></SSID></SSIDConfig>
<MSM><security><authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption></authEncryption>
<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>secretpass</keyMaterial></sharedKey></security></MSM></WLANProfile>
EOF
umount "$w/mnt"; sync

# ---- whole disk: GPT with ESP + NTFS
esp_s=$(( 200 * 2048 )); c_s=$(( $(stat -c %s "$w/c.img") / 512 ))
truncate -s $(( (2048 + esp_s + c_s + 2048) * 512 )) "$w/disk.img"
c_start=$(( 2048 + esp_s ))
sgdisk -n 1:2048:+${esp_s} -t 1:ef00 -n 2:${c_start}:+${c_s} -t 2:0700 "$w/disk.img"
sgdisk -p "$w/disk.img"
dd if="$w/esp.img" of="$w/disk.img" bs=512 seek=2048 conv=notrunc,sparse status=none
dd if="$w/c.img" of="$w/disk.img" bs=512 seek="$c_start" conv=notrunc,sparse status=none
rm -f "$w/esp.img" "$w/c.img"
df -h "$w" | tail -n 1

# ---- virtual TPM 2.0 + UEFI firmware with Secure Boot on and Microsoft's keys, like a PC
tpm() {   # (re)started before each boot, as a PC's chip starts fresh; its state stays
  pkill swtpm 2>/dev/null || true; sleep 1
  swtpm socket --tpm2 --tpmstate dir="$w/tpm" --ctrl type=unixio,path="$w/tpm.sock" --daemon
}
OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.secboot.fd
cp /usr/share/OVMF/OVMF_VARS_4M.ms.fd "$w/vars.fd"
ESP="$w/disk.img@@$(( 2048 * 512 ))"   # the EFI partition inside the disk, for mtools

boot() {   # $1 = log file
  tpm
  timeout 1500 qemu-system-x86_64 -machine q35,smm=on -global driver=cfi.pflash01,property=secure,value=on \
    -m 3072 -smp 2 -display none -vga virtio -no-reboot \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" -drive if=pflash,format=raw,file="$w/vars.fd" \
    -drive file="$w/disk.img",format=raw,if=virtio \
    -chardev socket,id=chrtpm,path="$w/tpm.sock" -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0 \
    -serial file:"$1" -monitor none > /dev/null 2>&1 || true
}
echo "=== boot 1 (handoff + seal)"; boot "$w/serial1.log"; grep -aE "lightsout:|LIGHTSOUT-SELFTEST" "$w/serial1.log" || true
# were the system file and your own space closed cleanly by boot 1's shutdown? Then an
# update: a fresh system file in place (your own space, user.img, stays)
cdev=$(losetup -f --show -o $(( c_start * 512 )) --sizelimit $(( c_s * 512 )) "$w/disk.img")
mkdir -p "$w/mid"; ntfs-3g "$cdev" "$w/mid" && {
  for f in system user; do echo "=== $f.img after boot 1:"; e2fsck -fn "$w/mid/LightsOut/$f.img" 2>&1 | tail -n 3; done
  cp --sparse=never "$SYSIMG" "$w/mid/LightsOut/system.img"
  umount "$w/mid"; }
losetup -d "$cdev"
# a forced power-off leaves the EFI partition marked as not closed cleanly (FAT32: bit 1 of byte 65)
dirty_at=$(( 2048 * 512 + 65 ))
printf '\001' | dd of="$w/disk.img" bs=1 seek="$dirty_at" conv=notrunc status=none
echo "=== boot 2 (unseal; the EFI partition left marked by a forced power-off)"; boot "$w/serial2.log"; grep -aE "lightsout:|LIGHTSOUT-SELFTEST" "$w/serial2.log" || true
dirty_after=$(( $(od -An -tu1 -j "$dirty_at" -N1 "$w/disk.img") & 1 ))
try() {   # as the installer does: a kernel option on this PC's trial list, and on the boot line
  mtype -i "$ESP" ::/EFI/lightsout/tries > "$w/tries" 2>/dev/null || : > "$w/tries"
  echo "$1 try" >> "$w/tries"; mcopy -o -i "$ESP" "$w/tries" ::/EFI/lightsout/tries
  for d in lightsout fedora; do
    mtype -i "$ESP" "::/EFI/$d/grub.cfg" | sed "s|^\(linux .*\)\$|\1 $1|" > "$w/g.cfg"
    mcopy -o -i "$ESP" "$w/g.cfg" "::/EFI/$d/grub.cfg"
  done
}
# a new boot line changes the TPM's measurements, so, as at every switch from Windows,
# the key is handed over for this start
try lightsout.works=1
mcopy -o -i "$ESP" "$w/handoff.key" ::/EFI/lightsout/handoff.key
echo "=== boot 3 (an option on trial; the key handed over again)"; boot "$w/serial3.log"; grep -aE "lightsout:|LIGHTSOUT-SELFTEST" "$w/serial3.log" || true
try lightsout.fails=1
sgdisk -d 2 -n 2:${c_start}:+$(( c_s - 131072 )) -t 2:0700 "$w/disk.img" >/dev/null
echo "=== boot 4 (C: shorter than its file system)"; boot "$w/serial4.log"; grep -a "lightsout:" "$w/serial4.log" || true
# boot 4 started with a kernel option on trial and never came up: boot 5 takes it off again
sgdisk -d 2 -n 2:${c_start}:+${c_s} -t 2:0700 "$w/disk.img" >/dev/null
echo "=== boot 5 (after a start whose option on trial never came up)"; boot "$w/serial5.log"; grep -a "lightsout:" "$w/serial5.log" || true

# ---- checks
ok=1
check() { if grep -aq "$2" "$1"; then echo "PASS: $3"; else echo "FAIL: $3"; ok=0; fi; }
check "$w/serial1.log" "Secure boot enabled" "Secure Boot on: the signed chain started"
check "$w/serial1.log" "drive key sealed into the TPM" "boot 1 sealed the key"
check "$w/serial1.log" "LIGHTSOUT-SELFTEST: c-drive-write=ok" "boot 1 wrote to C:"
check "$w/serial1.log" "LIGHTSOUT-SELFTEST: done" "boot 1 reached the self-test"
check "$w/serial2.log" "drive key unsealed from the TPM" "boot 2 unsealed the key"
check "$w/serial1.log" "TPM locked until the next start" "boot 1 locked the TPM after sealing"
check "$w/serial2.log" "TPM locked until the next start" "boot 2 locked the TPM after unsealing"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: done" "boot 2 reached the self-test"
check "$w/serial2.log" "reboot: Restarting system" "Lights On restarts the PC (the last step, after C: is closed)"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: terminal=running" "the Lights Out terminal is on screen"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: iwd-networks=.*psk" "Wi-Fi network imported from Windows"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: split=ok" "project got its own Linux node_modules"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: home=/mnt/c/Users/tester per-system-local=yes" "home is the Windows user folder, programs kept per system"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: resume-claude=claude = claude --continue" "resume list shipped as data"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: failed-units=[[:space:]]*$" "every system service started (Wi-Fi included)"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: cd-windows-path=/mnt/c/users" "cd takes Windows paths"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: windows-commands=0 0 ok.exe 3" "Windows programs in shared settings are skipped quietly"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: watchdog=1min at-restart=30s kept-on-in-restart=0" "a frozen system, or a restart that hangs anywhere, is forced by the watchdog"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: splash=success/bgrt" "start-up shows the PC logo with a spinner"
check "$w/serial1.log" "LIGHTSOUT-SELFTEST: own-space=/mnt/c/LightsOut/user.img kept=new" "boot 1 made your own space (user.img)"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: own-space=/mnt/c/LightsOut/user.img kept=yes" "an update (a new system file) kept your own space and your packages"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: screen-restart=ok" "the emergency key brings a frozen screen back"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: program-in-tab-recorded=yes" "a program typed in a tab is recorded right away"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: tabs-after-restart=all " "after a crash or the emergency key, the tabs come back"
check "$w/serial2.log" "EFI partition checked" "a start after a forced power-off checks the EFI partition"
if [ "$dirty_after" = 0 ]; then echo "PASS: the EFI partition is no longer marked (no warning at every start)"
else echo "FAIL: the EFI partition is still marked as not closed cleanly"; ok=0; fi
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: api: PANE  ROOM .*TAB  STATE" "control commands work inside Lights Out"
check "$w/serial4.log" "C: opened read-only by the driver" "boot 4 noticed C: opened read-only"
check "$w/serial4.log" "The system file needs a writable drive" "boot 4 stopped instead of running read-only"
check "$w/serial4.log" "on screen: Windows didn.t fully shut down" "boot 4 told you why, in one line"
check "$w/serial4.log" "shown under the spinner" "the starter's splash shows that line"
# the records on the EFI partition: every start, and boot 3's restart to its last step
for f in start restart; do mtype -i "$ESP" "::/EFI/lightsout/results/$f.log" > "$w/$f.log" 2>/dev/null || true; done
check "$w/start.log" "TPM locked until the next start" "every start leaves its record on the EFI partition"
check "$w/restart.log" "closing the system" "Lights On starts the restart record first"
check "$w/restart.log" "  kernel: " "the record keeps the kernel's last messages"
check "$w/restart.log" "programs stopped, systemd closed what it could" "the record marks the end of systemd's part"
check "$w/restart.log" "now: reboot (a PC still on after 3 s is restarted at once" "a restart records each step on the EFI partition, to the last one (with the 3-second fallback set)"
# kernel options on trial: kept after a start that came up, taken off after one that didn't
check "$w/serial3.log" "drive key sealed into the TPM" "after a new boot line, the key handed over is sealed anew"
check "$w/serial3.log" "LIGHTSOUT-SELFTEST: tries: lightsout.works=1 on on-this-start=lightsout.works=1" "an option on trial whose start came up is kept"
check "$w/serial4.log" "trying lightsout.fails=1" "the start using an option on trial marks it"
check "$w/serial5.log" "on screen: A setting Lights Out was trying didn" "a start after one that never came up says so, in one line"
mtype -i "$ESP" ::/EFI/lightsout/tries > "$w/tries" 2>/dev/null || true
mtype -i "$ESP" ::/EFI/fedora/grub.cfg > "$w/grub-after.cfg" 2>/dev/null || true
check "$w/tries" "lightsout.fails=1 off" "that option is off for good"
if grep -q "lightsout.works=1" "$w/grub-after.cfg" && ! grep -q "lightsout.fails=1" "$w/grub-after.cfg"; then
  echo "PASS: the boot line keeps the option that worked and drops the one that didn't"
else echo "FAIL: boot line after the tries: $(grep ^linux "$w/grub-after.cfg")"; ok=0; fi
cdev=$(losetup -f --show -o $(( c_start * 512 )) --sizelimit $(( c_s * 512 )) "$w/disk.img")
if ntfs-3g.probe --readwrite "$cdev"; then echo "PASS: C: closed cleanly"; else echo "FAIL: C: left dirty (code $?)"; ok=0; fi
losetup -d "$cdev"
grep -a "LIGHTSOUT-SELFTEST" "$w/serial2.log" > "$here/selftest-report.txt" || true
[ "$ok" = 1 ] && echo "ALL PASSED" || { echo "SOME CHECKS FAILED"; tail -n 80 "$w/serial2.log"; exit 1; }
