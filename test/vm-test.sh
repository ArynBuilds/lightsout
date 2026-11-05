#!/usr/bin/env bash
# Boots the real starter + system image in a virtual PC (UEFI with Secure Boot, virtual TPM)
# with a fake Windows "C:" (NTFS) and checks the whole chain, twice:
#   boot 1: one-time key handoff gets sealed into the TPM, system boots, writes to C:
#   boot 2: key is unsealed from the TPM, system boots again
#   boot 3: the partition is cut 64 MB shorter than the file system says (as on a PC where
#           the partition shrank), so the driver can only open C: read-only: it must stop
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

# ---- fake C: (NTFS) holding LightsOut\system.img, a Wi-Fi profile and the user name
sys_mb=$(( $(stat -c %s "$SYSIMG") / 1048576 ))
truncate -s "$(( sys_mb + 600 ))M" "$w/c.img"; mkntfs -q -F -f -L Windows "$w/c.img"
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
swtpm socket --tpm2 --tpmstate dir="$w/tpm" --ctrl type=unixio,path="$w/tpm.sock" --daemon
OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.secboot.fd
cp /usr/share/OVMF/OVMF_VARS_4M.ms.fd "$w/vars.fd"

boot() {   # $1 = log file
  timeout 1500 qemu-system-x86_64 -machine q35,smm=on -global driver=cfi.pflash01,property=secure,value=on \
    -m 3072 -smp 2 -display none -vga virtio -no-reboot \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" -drive if=pflash,format=raw,file="$w/vars.fd" \
    -drive file="$w/disk.img",format=raw,if=virtio \
    -chardev socket,id=chrtpm,path="$w/tpm.sock" -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis,tpmdev=tpm0 \
    -serial file:"$1" -monitor none > /dev/null 2>&1 || true
}
echo "=== boot 1 (handoff + seal)"; boot "$w/serial1.log"; grep -aE "lightsout:|LIGHTSOUT-SELFTEST" "$w/serial1.log" || true
# was the system file closed cleanly by boot 1's shutdown? (read in place, no copy)
cdev=$(losetup -f --show -r -o $(( c_start * 512 )) --sizelimit $(( c_s * 512 )) "$w/disk.img")
mkdir -p "$w/mid"; ntfs-3g -o ro "$cdev" "$w/mid" && {
  echo "=== system file after boot 1:"; e2fsck -fn "$w/mid/LightsOut/system.img" 2>&1 | tail -n 3
  umount "$w/mid"; }
losetup -d "$cdev"
pkill swtpm 2>/dev/null || true; sleep 1
swtpm socket --tpm2 --tpmstate dir="$w/tpm" --ctrl type=unixio,path="$w/tpm.sock" --daemon
echo "=== boot 2 (unseal)"; boot "$w/serial2.log"; grep -aE "lightsout:|LIGHTSOUT-SELFTEST" "$w/serial2.log" || true
sgdisk -d 2 -n 2:${c_start}:+$(( c_s - 131072 )) -t 2:0700 "$w/disk.img" >/dev/null
pkill swtpm 2>/dev/null || true; sleep 1
swtpm socket --tpm2 --tpmstate dir="$w/tpm" --ctrl type=unixio,path="$w/tpm.sock" --daemon
echo "=== boot 3 (C: shorter than its file system)"; boot "$w/serial3.log"; grep -a "lightsout:" "$w/serial3.log" || true

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
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: watchdog=1min at-restart=30s" "a frozen system, or a restart that hangs, is forced by the watchdog"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: splash=success/bgrt" "start-up shows the PC logo with a spinner"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: screen-restart=ok" "the emergency key brings a frozen screen back"
check "$w/serial2.log" "LIGHTSOUT-SELFTEST: api: PANE  ROOM .*TAB  STATE" "control commands work inside Lights Out"
check "$w/serial3.log" "C: opened read-only by the driver" "boot 3 noticed C: opened read-only"
check "$w/serial3.log" "The system file needs a writable drive" "boot 3 stopped instead of running read-only"
check "$w/serial3.log" "on screen: Windows didn.t fully shut down" "boot 3 told you why, in one line"
check "$w/serial3.log" "shown under the spinner" "the starter's splash shows that line"
cdev=$(losetup -f --show -o $(( c_start * 512 )) --sizelimit $(( c_s * 512 )) "$w/disk.img")
if ntfs-3g.probe --readwrite "$cdev"; then echo "PASS: C: closed cleanly"; else echo "FAIL: C: left dirty (code $?)"; ok=0; fi
losetup -d "$cdev"
grep -a "LIGHTSOUT-SELFTEST" "$w/serial2.log" > "$here/selftest-report.txt" || true
[ "$ok" = 1 ] && echo "ALL PASSED" || { echo "SOME CHECKS FAILED"; tail -n 80 "$w/serial2.log"; exit 1; }
