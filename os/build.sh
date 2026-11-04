#!/usr/bin/env bash
# Builds the Lights Out system: a Debian root filesystem packed into one ext4
# image file (system.img) that lives on the user's C: drive. Everything the
# user installs or logs into later is stored inside it.
# Runs as root in a debian:trixie container. Output: out/system.img.zst
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/out"; root="$here/rootfs"
rm -rf "$out" "$root"; mkdir -p "$out"

pkgs=(
  # base system
  systemd systemd-sysv udev dbus libpam-systemd sudo bash-completion less nano vim-tiny
  ca-certificates curl wget git openssh-client gnupg jq ripgrep unzip xz-utils zstd
  procps psmisc file htop tmux man-db locales
  # agent toolbox (more via `sudo apt install`, kept on disk)
  nodejs npm python3 python3-venv python3-pip pipx gh
  # network: iwd does Wi-Fi + DHCP on its own
  iwd wireless-regdb systemd-resolved systemd-timesyncd
  # drive unlock + TPM
  cryptsetup-bin tpm2-tools ntfs-3g
  # screen: cage shows exactly one full-screen app (our terminal)
  sway swaybg foot wlopm brightnessctl libxkbcommon0 libwayland-client0 libgl1-mesa-dri libegl1 fonts-jetbrains-mono fonts-noto-color-emoji fonts-dejavu-core fonts-symbola
  # kernel modules + firmware (the starter boots this same kernel)
  linux-image-amd64 linux-cpupower
  firmware-linux-free firmware-misc-nonfree firmware-iwlwifi firmware-intel-graphics
  firmware-sof-signed firmware-realtek firmware-atheros firmware-mediatek firmware-brcm80211
  firmware-amd-graphics
)

mmdebstrap --variant=minbase --components="main contrib non-free non-free-firmware" \
  --include="$(IFS=,; echo "${pkgs[*]}")" \
  --customize-hook="cp -a $here/overlay/. \"\$1\"/" \
  --customize-hook="chroot \"\$1\" bash /usr/local/lib/lightsout/setup.sh" \
  trixie "$root" http://deb.debian.org/debian

# Our terminal (built separately) if present
for b in lightsout lightsout-app; do
  if [ -x "$here/../term-bin/$b" ]; then install -m755 "$here/../term-bin/$b" "$root/usr/local/bin/$b"; fi
done

# Kernel + modules for the starter come from this exact system
mkdir -p "$out/boot"
cp "$root"/boot/vmlinuz-* "$out/boot/vmlinuz"
ls "$root/lib/modules" > "$out/boot/kernel-version"

# Pack: used size + 1.5 GB headroom (grows later from inside Lights Out)
used_mb=$(du -sm "$root" | cut -f1)
size_mb=$(( used_mb + 1536 ))
rm -f "$out/system.img"
mkfs.ext4 -q -L lightsout -d "$root" "$out/system.img" "${size_mb}M"
# gzip: Windows can unpack it without extra tools
pigz -6 -k -c "$out/system.img" > "$out/system.img.gz"
(cd "$out" && sha256sum system.img.gz > system.img.gz.sha256)
echo "rootfs: ${used_mb} MB, image: ${size_mb} MB, compressed: $(du -h "$out/system.img.gz" | cut -f1)"
