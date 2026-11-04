#!/bin/sh
# Lights Out installer for Linux:  curl -fsSL https://lightsout.sh | sh
# Installs the terminal (`lightsout`) and Lights Out mode: the desktop stops,
# only Lights Out runs; Lights On brings the desktop back (no restart).
set -eu
BASE="${LIGHTSOUT_BASE:-https://github.com/ArynBuilds/lightsout/releases/latest/download}"
say() { printf '\033[1m%s\033[0m\n' "$*"; }
stop() { printf 'Lights Out: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null || stop "needs curl"
SUDO=""; [ "$(id -u)" = 0 ] || SUDO=sudo
ME="${SUDO_USER:-$(id -un)}"
[ "$ME" != root ] || stop "run it as your normal user (it asks for sudo when needed)"

install_mac() {
  say "Downloading Lights Out for Mac"
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  curl -fL --progress-bar "$BASE/lightsout-macos.tar.gz" -o "$tmp/lo.tgz"
  tar -xzf "$tmp/lo.tgz" -C "$tmp"
  src="$tmp/lightsout-macos"
  say "Installing (asks for your Mac password once)"
  $SUDO mkdir -p /usr/local/bin /usr/local/lib/lightsout
  $SUDO install -m 755 "$src/bin/lightsout" "$src/bin/lightsout-app" /usr/local/bin/
  $SUDO install -m 755 "$src/lib/switch" "$src/lib/logout-later" "$src/lib/uninstall.sh" /usr/local/lib/lightsout/
  home=$(dscl . -read "/Users/$ME" NFSHomeDirectory | awk '{print $2}')
  # Log-out mode (agents in a background service while you're logged out) is
  # still being verified; LIGHTSOUT_MAC_LOGOUT=1 turns it on. Without it, Lights
  # Out stays signed in and slows everything else down.
  if [ "${LIGHTSOUT_MAC_LOGOUT:-0}" = 1 ]; then
    # the background service that keeps agents running while you're logged out
    $SUDO tee /Library/LaunchDaemons/sh.lightsout.agents.plist >/dev/null <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>sh.lightsout.agents</string>
<key>UserName</key><string>$ME</string>
<key>ProgramArguments</key><array><string>/usr/local/bin/lightsout-app</string><string>--headless</string></array>
<key>EnvironmentVariables</key><dict><key>HOME</key><string>$home</string><key>USER</key><string>$ME</string><key>SHELL</key><string>$(dscl . -read "/Users/$ME" UserShell | awk '{print $2}')</string><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
<key>WorkingDirectory</key><string>$home</string>
<key>RunAtLoad</key><false/>
<key>ProcessType</key><string>Standard</string>
<key>StandardErrorPath</key><string>/tmp/lightsout-agents.log</string>
</dict></plist>
P
    $SUDO chmod 644 /Library/LaunchDaemons/sh.lightsout.agents.plist
    $SUDO chown root:wheel /Library/LaunchDaemons/sh.lightsout.agents.plist
    # loaded but never started on its own: Lights Out starts it on demand
    $SUDO launchctl bootout system/sh.lightsout.agents 2>/dev/null || true
    $SUDO launchctl bootstrap system /Library/LaunchDaemons/sh.lightsout.agents.plist || echo "Note: the background service will be loaded at the next restart."
    # signing back in after Lights Out: the app opens and takes your tabs back
    mkdir -p "$home/Library/LaunchAgents"
    cat > "$home/Library/LaunchAgents/sh.lightsout.return.plist" <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>sh.lightsout.return</string>
<key>ProgramArguments</key><array><string>/usr/local/bin/lightsout-app</string><string>--if-returning</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
P
  fi
  echo "$ME ALL=(root) NOPASSWD: /usr/local/lib/lightsout/switch" > "$tmp/sudoers"
  $SUDO visudo -cf "$tmp/sudoers" >/dev/null && $SUDO install -m 440 "$tmp/sudoers" /etc/sudoers.d/lightsout
  # a small app so Lights Out is in Launchpad, Spotlight and the Dock
  app="/Applications/Lights Out.app/Contents"
  $SUDO mkdir -p "$app/MacOS"
  printf '#!/bin/sh\nexec /usr/local/bin/lightsout-app "$@"\n' | $SUDO tee "$app/MacOS/lightsout" >/dev/null
  $SUDO chmod 755 "$app/MacOS/lightsout"
  $SUDO tee "$app/Info.plist" >/dev/null <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Lights Out</string>
<key>CFBundleIdentifier</key><string>sh.lightsout.app</string>
<key>CFBundleExecutable</key><string>lightsout</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.2.1</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
P
  command -v blueutil >/dev/null || echo "Tip: 'brew install blueutil' lets Lights Out switch Bluetooth off too."
  say "Lights Out is installed."
  echo "Open it with:  lightsout   (or Lights Out in Launchpad)"
  echo "The Lights Out button (bottom left) makes the Mac quiet; Lights On brings everything back."
  echo "Remove it with:  lightsout uninstall"
  exit 0
}

case "$(uname -s)" in
  Linux) ;;
  Darwin) install_mac ;;
  *) stop "this system isn't supported yet. On Windows run: irm https://lightsout.sh | iex" ;;
esac
[ "$(uname -m)" = x86_64 ] || stop "needs a 64-bit Intel or AMD PC for now"

say "Downloading Lights Out"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
curl -fL --progress-bar "$BASE/lightsout-linux-x64.tar.gz" -o "$tmp/lo.tgz"
tar -xzf "$tmp/lo.tgz" -C "$tmp"
src="$tmp/lightsout-linux-x64"

say "Installing the terminal"
$SUDO install -d /usr/local/lib/lightsout
$SUDO install -m 755 "$src/bin/lightsout" "$src/bin/lightsout-app" /usr/local/bin/
$SUDO install -m 755 "$src/lib/switch" "$src/lib/kiosk.sh" "$src/lib/uninstall.sh" /usr/local/lib/lightsout/
$SUDO install -m 755 "$src/sbin/lightsout-power" "$src/sbin/lightsout-cores" /usr/local/sbin/

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
