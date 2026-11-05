#!/usr/bin/env bash
# Builds the small starter for the EFI partition, using the kernel and modules
# of the system image built by ../os/build.sh (so versions always match).
# Output: out/EFI/lightsout/{shimx64.efi,mmx64.efi,grubx64.efi,grub.cfg,vmlinuz,starter.img}
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sys="$here/../os/rootfs"            # system root filesystem from the os build
out="$here/out"; root="$here/root"
rm -rf "$out" "$root"; mkdir -p "$out/EFI/lightsout" "$out/EFI/fedora" "$root"
KVER=$(ls "$sys/lib/modules" | sort -V | tail -n1)

# ---- signed boot chain: Microsoft-signed shim -> Fedora-signed grub -> Fedora-signed kernel
# (the packages were downloaded and their signatures checked by ../os/build.sh)
fed="$here/../os/out/fedora"
for r in "$fed"/shim-x64-*.rpm "$fed"/grub2-efi-x64-*.rpm; do (cd "$fed" && rpm2cpio "$r" | cpio -idm --quiet "*/EFI/fedora/*"); done
for f in shimx64.efi mmx64.efi grubx64.efi; do cp "$(find "$fed/usr/lib/efi" -path "*/EFI/fedora/$f" | head -n 1)" "$out/EFI/lightsout/$f"; done
cp "$sys/boot/vmlinuz-$KVER" "$out/EFI/lightsout/vmlinuz"
cat > "$out/EFI/lightsout/grub.cfg" <<'EOF'
set timeout=0
search --no-floppy --file --set=root /EFI/lightsout/starter.img
linux /EFI/lightsout/vmlinuz quiet splash loglevel=3 systemd.show_status=false vt.global_cursor_default=0 plymouth.use-simpledrm nmi_watchdog=0 pcie_aspm.policy=powersupersave modprobe.blacklist=nouveau,nvidia,nvidia_drm,nvidia_modeset selinux=0
initrd /EFI/lightsout/starter.img
boot
EOF
cp "$out/EFI/lightsout/grub.cfg" "$out/EFI/fedora/grub.cfg"   # Fedora's grub reads \EFI\fedora\grub.cfg

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
# the splash, for the one line shown when something stops the start (see init): the
# system's own copy, with the libraries and plug-ins it needs as the system resolves them
from_sys() {
  for f; do
    mkdir -p "$root$(dirname "$f")"; cp -L "$sys$f" "$root$f"
    chroot "$sys" ldd "$f" | grep -oE '/[^ ]+' | while read -r l; do
      [ -e "$root$l" ] || { mkdir -p "$root$(dirname "$l")"; cp -L "$sys$l" "$root$l"; }
    done
  done
}
P=/usr/lib/x86_64-linux-gnu/plymouth
from_sys /usr/sbin/plymouthd /usr/bin/plymouth $P/two-step.so $P/label-freetype.so $P/renderers/drm.so $P/renderers/frame-buffer.so
mkdir -p "$root/usr/share/plymouth/themes" "$root/etc/plymouth" "$root/usr/share/fonts"
cp -r "$sys/usr/share/plymouth/themes/bgrt" "$sys/usr/share/plymouth/themes/spinner" "$root/usr/share/plymouth/themes/"
cp "$sys/usr/share/plymouth/plymouthd.defaults" "$root/usr/share/plymouth/"
cp "$sys/etc/plymouth/plymouthd.conf" "$root/etc/plymouth/"
cp "$sys/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf" "$root/usr/share/fonts/Plymouth.ttf"   # the label's font
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
for f in modules.order modules.builtin modules.builtin.modinfo; do
  [ -f "$sys/lib/modules/$KVER/$f" ] && cp "$sys/lib/modules/$KVER/$f" "$root/lib/modules/$KVER/"
done
depmod -b "$root" "$KVER"
echo "starter modules: $(find "$root/lib/modules" -name '*.ko*' | wc -l)"

(cd "$root" && find . | cpio -o -H newc --quiet | zstd -19 -T0 -q -o "$out/EFI/lightsout/starter.img")
du -h "$out"/EFI/lightsout/*
echo "starter total: $(du -sh "$out" | cut -f1)"
