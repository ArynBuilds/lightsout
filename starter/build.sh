#!/usr/bin/env bash
# Builds the small starter for the EFI partition, using the kernel and modules
# of the system image built by ../os/build.sh (so versions always match).
# Output: out/EFI/lightsout/{shimx64.efi,mmx64.efi,grubx64.efi,grub.cfg,vmlinuz,starter.img}
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sys="$here/../os/rootfs"            # system root filesystem from the os build
out="$here/out"; root="$here/root"
rm -rf "$out" "$root"; mkdir -p "$out/EFI/lightsout" "$out/EFI/debian" "$root"
KVER=$(ls "$sys/lib/modules" | sort -V | tail -n1)

# ---- signed boot chain: Microsoft-signed shim -> Debian-signed grub -> Debian-signed kernel
cp /usr/lib/shim/shimx64.efi.signed "$out/EFI/lightsout/shimx64.efi"
cp "$(ls /usr/lib/shim/mmx64.efi.signed /usr/lib/shim/mmx64.efi 2>/dev/null | head -n1)" "$out/EFI/lightsout/mmx64.efi"
cp /usr/lib/grub/x86_64-efi-signed/grubx64.efi.signed "$out/EFI/lightsout/grubx64.efi"
cp "$sys/boot/vmlinuz-$KVER" "$out/EFI/lightsout/vmlinuz"
cat > "$out/EFI/lightsout/grub.cfg" <<'EOF'
set timeout=0
search --no-floppy --file --set=root /EFI/lightsout/starter.img
linux /EFI/lightsout/vmlinuz quiet loglevel=3 nmi_watchdog=0 pcie_aspm.policy=powersupersave modprobe.blacklist=nouveau,nvidia,nvidia_drm,nvidia_modeset reboot=efi
initrd /EFI/lightsout/starter.img
boot
EOF
cp "$out/EFI/lightsout/grub.cfg" "$out/EFI/debian/grub.cfg"   # Debian's grub reads \EFI\debian\grub.cfg

# ---- userland
mkdir -p "$root"/{bin,sbin,etc,lib,lib64,proc,sys,dev,run,tmp,esp,mnt/c,newroot,usr/lib}
cp /bin/busybox "$root/bin/busybox"
for app in $(/bin/busybox --list); do [ -e "$root/bin/$app" ] || ln -s busybox "$root/bin/$app"; done
copy_lib() { mkdir -p "$root$(dirname "$1")"; cp -L "$1" "$root$1"; }
copy_bin() {
  rm -f "$root/bin/$(basename "$1")"; cp -L "$1" "$root/bin/$(basename "$1")"
  ldd "$1" | grep -oE '/[^ ]+' | while read -r lib; do copy_lib "$lib"; done
}
for b in /usr/bin/kmod /usr/sbin/cryptsetup /usr/sbin/blkid /usr/bin/tpm2; do copy_bin "$b"; done
for t in tpm2_load tpm2_unseal tpm2_create tpm2_createpolicy tpm2_startauthsession tpm2_policypcr \
         tpm2_flushcontext tpm2_readpublic tpm2_createprimary tpm2_pcrread tpm2_pcrextend; do ln -sf tpm2 "$root/bin/$t"; done
# the TPM library loads its device driver at run time (ldd can't see it)
for l in /usr/lib/x86_64-linux-gnu/libtss2-tcti-device.so* /usr/lib/x86_64-linux-gnu/libtss2-tctildr.so*; do copy_lib "$l"; done
for t in modprobe insmod lsmod depmod modinfo; do rm -f "$root/bin/$t"; ln -s kmod "$root/bin/$t"; done
ln -sf ../bin/kmod "$root/sbin/modprobe"
install -m755 "$here/init" "$root/init"
install -m755 "$here/lo-shutdown" "$root/bin/lo-shutdown"

# ---- modules: storage, encryption, file systems, TPM, keyboards (no Wi-Fi, no GPU)
M="$sys/lib/modules/$KVER/kernel"
dirs="drivers/nvme drivers/pci/controller drivers/ata drivers/scsi drivers/md drivers/char/tpm drivers/hid
      drivers/input drivers/usb/host drivers/usb/core drivers/usb/storage drivers/mmc drivers/block
      drivers/acpi drivers/platform/x86/intel drivers/pinctrl drivers/i2c drivers/mfd drivers/virtio
      fs/ntfs3 fs/ext4 fs/jbd2 fs/fat fs/nls fs/efivarfs crypto arch/x86/crypto lib"
mods=$(for d in $dirs; do [ -d "$M/$d" ] && find "$M/$d" -name '*.ko*'; done \
       | xargs -r -n1 basename | sed -E 's/\.ko(\.(xz|zst|gz))?$//' | sort -u)
for m in $mods; do
  modprobe -d "$sys" -S "$KVER" -D "$m" 2>/dev/null | awk '/^insmod/ {print $2}'
done | sort -u | while read -r f; do mkdir -p "$root$(dirname "${f#$sys}")"; cp "$f" "$root${f#$sys}"; done
cp "$sys/lib/modules/$KVER"/modules.{order,builtin,builtin.modinfo} "$root/lib/modules/$KVER/"
depmod -b "$root" "$KVER"
echo "starter modules: $(find "$root/lib/modules" -name '*.ko*' | wc -l)"

(cd "$root" && find . | cpio -o -H newc --quiet | zstd -19 -T0 -q -o "$out/EFI/lightsout/starter.img")
du -h "$out"/EFI/lightsout/*
echo "starter total: $(du -sh "$out" | cut -f1)"
