#!/bin/sh
# Web plugin (Linux, Mac): draws a web page into its panel with Chrome, Chromium or Edge,
# without a window, only when asked (open, reload, the panel's size changed). Nothing keeps
# running.
state=$LIGHTSOUT_PLUGIN_STATE
url_file=$state/url.txt
size_file=$state/size.txt
part=web.page

say() { lightsout part set "$part" "$1" >/dev/null 2>&1; }

browser() {
  for b in chromium chromium-browser google-chrome google-chrome-stable microsoft-edge \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"; do
    if command -v "$b" >/dev/null 2>&1 || [ -x "$b" ]; then echo "$b"; return; fi
  done
}

# a number from the event's JSON
field() { echo "$LIGHTSOUT_EVENT_JSON" | sed -n "s/.*\"$1\":\([0-9]*\).*/\1/p"; }

# draws the page at the panel's size (known once the panel shows: part.resized)
draw() {
  [ -s "$url_file" ] || return
  url=$(cat "$url_file")
  if [ ! -s "$size_file" ]; then say "Loading $url ..."; return; fi
  read -r w h < "$size_file"
  if [ "$w" = cells ]; then
    say "$url
The page shows in the Lights Out window (lightsout --window)."
    return
  fi
  b=$(browser)
  if [ -z "$b" ]; then
    say "No Chrome, Chromium or Edge to draw the page with.
lightsout do web.browser opens it in your browser."
    return
  fi
  png=$state/page.png
  rm -f "$png"
  "$b" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
    --user-data-dir="$state/profile" --window-size="$w,$h" --virtual-time-budget=4000 \
    --screenshot="$png" "$url" >/dev/null 2>&1
  if [ -s "$png" ]; then lightsout part image "$part" "$png" >/dev/null 2>&1
  else say "Couldn't draw $url"; fi
}

case "$1" in
  open)
    url=$(echo "$LIGHTSOUT_ARG" | sed 's/^ *//; s/ *$//')
    [ -n "$url" ] || url=$(cat "$url_file" 2>/dev/null)
    if [ -z "$url" ]; then say "lightsout do web.open https://..."; exit 0; fi
    case "$url" in http://* | https://*) ;; *) url="https://$url" ;; esac
    echo "$url" > "$url_file"
    if [ -s "$size_file" ]; then draw; else say "Loading $url ..."; fi
    ;;
  reload) draw ;;
  close)
    lightsout part clear "$part" >/dev/null 2>&1
    rm -f "$size_file"
    ;;
  browser | clicked)
    if [ "$1" = clicked ]; then
      case "$LIGHTSOUT_EVENT_JSON" in *'"part":"web.page"'*) ;; *) exit 0 ;; esac
    fi
    url=$(cat "$url_file" 2>/dev/null)
    [ -n "$url" ] || exit 0
    if command -v open >/dev/null 2>&1 && [ "$(uname)" = Darwin ]; then open "$url"
    else xdg-open "$url" >/dev/null 2>&1; fi
    ;;
  resized)
    case "$LIGHTSOUT_EVENT_JSON" in *'"part":"web.page"'*) ;; *) exit 0 ;; esac
    w=$(field width); h=$(field height); c=$(field cols)
    # in a terminal the size is in cells: no picture shows there
    if [ "$w" = "$c" ]; then echo "cells 0" > "$size_file"; else echo "$w $h" > "$size_file"; fi
    draw
    ;;
esac
