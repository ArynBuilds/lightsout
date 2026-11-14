#!/bin/sh
# A frame: a picture drawn at the part's size, again whenever its size changes. This one is
# plain grey, written as a PPM (any language can write one: a short header, then RGB bytes).
case "$LIGHTSOUT_EVENT_JSON" in *'"part":"test-plugin.frame"'*) ;; *) exit 0 ;; esac
w=$(echo "$LIGHTSOUT_EVENT_JSON" | sed 's/.*"width":\([0-9]*\).*/\1/')
h=$(echo "$LIGHTSOUT_EVENT_JSON" | sed 's/.*"height":\([0-9]*\).*/\1/')
f="$LIGHTSOUT_PLUGIN_STATE/frame.ppm"
printf 'P6\n%s %s\n255\n' "$w" "$h" > "$f"
head -c $((w * h * 3)) /dev/zero | tr '\000' '\200' >> "$f"
lightsout part image frame "$f"
