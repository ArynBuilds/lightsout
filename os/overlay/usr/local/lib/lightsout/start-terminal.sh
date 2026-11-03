#!/bin/bash
# sway as a kiosk: no bars, no borders, one full-screen app (our terminal).
# sway (unlike cage) can switch the screen off: wlopm / swaymsg "output * power off".
. /usr/local/lib/lightsout/lightsout-env.sh
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
term=/usr/local/bin/lightsout-app
[ -x "$term" ] || term="foot --font='JetBrains Mono:size=11'"

# keyboard layout from Windows (installer writes e.g. "tr" or "us")
layout=$(tr -dc 'a-z,' < /mnt/c/LightsOut/config/keyboard 2>/dev/null)
[ -n "$layout" ] || layout=us

conf="$XDG_RUNTIME_DIR/lightsout-sway.conf"
cat > "$conf" <<CONF
default_border none
default_floating_border none
hide_edge_borders both
focus_follows_mouse no
seat * hide_cursor 4000
output * bg #000000 solid_color
input type:keyboard xkb_layout $layout
input type:touchpad {
  tap enabled
  natural_scroll enabled
}
for_window [app_id=".*"] fullscreen enable
# closing the lid switches the screen off; opening it switches it back on
bindswitch --reload --locked lid:on output * power off
bindswitch --reload --locked lid:off output * power on
exec $term
CONF
exec sway --unsupported-gpu -c "$conf"
