#!/bin/sh
# Lights Out on a Linux PC: a full-screen sway that shows only the Lights Out terminal.
export LIGHTSOUT_MODE=inplace XDG_SESSION_TYPE=wayland
[ -n "${XDG_RUNTIME_DIR:-}" ] || export XDG_RUNTIME_DIR="/run/user/$(id -u)"
layout=$(localectl status 2>/dev/null | sed -n 's/.*X11 Layout: *//p' | head -n1)
conf="$XDG_RUNTIME_DIR/lightsout-sway.conf"
cat > "$conf" <<CONF
default_border none
hide_edge_borders both
focus_follows_mouse no
seat * hide_cursor 4000
output * bg #000000 solid_color
input type:keyboard xkb_layout ${layout:-us}
input type:touchpad {
  tap enabled
  natural_scroll enabled
}
for_window [app_id=".*"] fullscreen enable
bindswitch --reload --locked lid:on output * power off
bindswitch --reload --locked lid:off output * power on
exec sh -c 'lightsout-app; swaymsg exit'
CONF
# lid closed = screen off, never sleep (while Lights Out runs)
systemd-inhibit --what=handle-lid-switch:sleep:idle --who="Lights Out" --why="agents keep running" --mode=block sway -c "$conf"
# the app was closed: back to the desktop
sudo -n /usr/local/lib/lightsout/switch in
