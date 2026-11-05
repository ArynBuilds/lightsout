#!/bin/bash
# sway as a kiosk: no bars, no borders, one full-screen app (our terminal).
# sway (unlike cage) can switch the screen off: wlopm / swaymsg "output * power off".
. /usr/local/lib/lightsout/lightsout-env.sh
export XDG_RUNTIME_DIR="/run/user/$(id -u)"
term=/usr/local/bin/lightsout-app
[ -x "$term" ] || term=foot   # (plain: it goes inside sh -c '...' below)

# keyboard layout from Windows (installer writes e.g. "tr" or "us")
layout=$(tr -dc 'a-z,' < /mnt/c/LightsOut/config/keyboard 2>/dev/null)
[ -n "$layout" ] || layout=us
# the lowest brightness for the keys, 1%, as a number (brightnessctl takes it only as --min-value=N)
dim=$(( $(brightnessctl m 2>/dev/null || echo 100) / 100 + 1 ))

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
bindswitch --reload --locked lid:on exec sh -c 'swaymsg "output * power off"; lightsout screen off'
bindswitch --reload --locked lid:off exec sh -c 'swaymsg "output * power on"; lightsout screen on'
# a frozen screen: Ctrl+Alt+Backspace stops the terminal, which restarts it (your tabs come back)
bindsym --locked Ctrl+Alt+BackSpace exec pkill -KILL -x lightsout-app
# the brightness keys (never fully dark: 1% is the lowest)
bindsym --locked XF86MonBrightnessUp exec brightnessctl -q set 5%+
bindsym --locked XF86MonBrightnessDown exec brightnessctl -q --min-value=$dim set 5%-
exec /usr/local/lib/lightsout/lowest-refresh
# the terminal ended (closed, crashed, stopped by the key): sway ends too, so the service restarts both
exec sh -c '$term; swaymsg exit'
CONF
exec sway --unsupported-gpu -c "$conf"
