#!/bin/sh
# lowest-refresh: a 240 Hz panel goes to its 60 Hz mode at the same size (the 30 Hz mode
# is another size, so it is not taken); a screen that is off is left alone.
set -eu
here=$(cd "$(dirname "$0")" && pwd); fake=$(mktemp -d)
cat > "$fake/swaymsg" <<'EOF'
#!/bin/sh
[ "$1" = -t ] && exec echo '[{"name":"eDP-1","active":true,"current_mode":{"width":2560,"height":1600,"refresh":240000},"modes":[{"width":1920,"height":1200,"refresh":30000},{"width":2560,"height":1600,"refresh":240000},{"width":2560,"height":1600,"refresh":60000}]},{"name":"HDMI-A-1","active":false,"current_mode":{"width":0,"height":0,"refresh":0},"modes":[]}]'
echo "$*" >> "$(dirname "$0")/ran"
EOF
chmod +x "$fake/swaymsg"
PATH="$fake:$PATH" sh "$here/../os/overlay/usr/local/lib/lightsout/lowest-refresh"
if [ "$(cat "$fake/ran")" = 'output "eDP-1" mode 2560x1600@60Hz' ]; then echo "PASS: screens run at their lowest refresh rate"
else echo "FAIL: lowest-refresh ran: $(cat "$fake/ran")"; exit 1; fi
