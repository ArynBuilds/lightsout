#!/bin/sh
# Lights Out installer for Linux:  curl -fsSL https://lightsout.sh | sh
# Installs the terminal (`lightsout`) and Lights Out mode: the desktop stops,
# only Lights Out runs; Lights On brings the desktop back (no restart).
set -eu
BASE="${LIGHTSOUT_BASE:-https://github.com/ArynBuilds/lightsout/releases/latest/download}"
say() { printf '\033[1m%s\033[0m\n' "$*"; }
stop() { printf 'Lights Out: %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in
  Linux) ;;
  Darwin) echo "Lights Out for Mac is coming next. Windows and Linux work today."; exit 0 ;;
  *) stop "this system isn't supported yet. On Windows run: irm https://lightsout.sh | iex" ;;
esac
[ "$(uname -m)" = x86_64 ] || stop "needs a 64-bit Intel or AMD PC for now"
command -v curl >/dev/null || stop "needs curl"
SUDO=""; [ "$(id -u)" = 0 ] || SUDO=sudo
ME="${SUDO_USER:-$(id -un)}"
[ "$ME" != root ] || stop "run it as your normal user (it asks for sudo when needed)"

say "Downloading Lights Out"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
curl -fL --progress-bar "$BASE/lightsout-linux-x64.tar.gz" -o "$tmp/lo.tgz"
tar -xzf "$tmp/lo.tgz" -C "$tmp"
src="$tmp/lightsout-linux-x64"

say "Installing the terminal"
$SUDO install -d /usr/local/lib/lightsout
$SUDO install -m 755 "$src/bin/lightsout" "$src/bin/lightsout-app" /usr/local/bin/
$SUDO install -m 755 "$src/lib/switch" "$src/lib/kiosk.sh" "$src/lib/uninstall.sh" /usr/local/lib/lightsout/
$SUDO install -m 755 "$src/sbin/lightsout-power" /usr/local/sbin/

home=$(getent passwd "$ME" | cut -d: -f6)
apps="$home/.local/share/applications"; auto="$home/.config/autostart"
mkdir -p "$apps" "$auto"
cat > "$apps/lightsout.desktop" <<D
[Desktop Entry]
Type=Application
Name=Lights Out
Comment=The terminal for AI agents
Exec=lightsout-app
Terminal=false
Categories=System;TerminalEmulator;
D
# back from Lights Out: reopen the tabs after signing in
cat > "$auto/lightsout-return.desktop" <<D
[Desktop Entry]
Type=Application
Name=Lights Out (reopen tabs)
Exec=sh -c 'test -f "\${XDG_STATE_HOME:-\$HOME/.local/state}/lightsout/handoff/tabs.json" && exec lightsout-app'
NoDisplay=true
D

say "Setting up Lights Out mode"
if [ ! -d /run/systemd/system ]; then
  echo "Lights Out mode needs systemd; the terminal works anyway."
  exit 0
fi
if ! command -v sway >/dev/null; then
  if command -v apt-get >/dev/null; then $SUDO apt-get install -y sway
  elif command -v dnf >/dev/null; then $SUDO dnf install -y sway
  elif command -v pacman >/dev/null; then $SUDO pacman -S --needed --noconfirm sway
  elif command -v zypper >/dev/null; then $SUDO zypper --non-interactive install sway
  fi || true
fi
command -v sway >/dev/null || echo "Couldn't install sway (needed for Lights Out mode). The terminal works anyway."
for u in "$src"/systemd/*; do
  sed "s/__USER__/$ME/" "$u" | $SUDO tee "/etc/systemd/system/$(basename "$u")" >/dev/null
done
echo "$ME ALL=(root) NOPASSWD: /usr/local/lib/lightsout/switch" > "$tmp/sudoers"
if $SUDO visudo -cf "$tmp/sudoers" >/dev/null 2>&1 || ! command -v visudo >/dev/null; then
  $SUDO install -m 440 "$tmp/sudoers" /etc/sudoers.d/lightsout
fi
$SUDO systemctl daemon-reload

say "Lights Out is installed."
echo "Open it with:  lightsout   (or from your apps menu)"
echo "The Lights Out button (bottom left) stops the desktop; Lights On brings it back."
echo "Remove it with:  lightsout uninstall"
