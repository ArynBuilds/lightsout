#!/bin/sh
# mac/guard puts the Mac's settings back only when Lights Out changed them and none
# of it runs any more; never while the app, its background copy or a switch is running.
set -eu
here=$(cd "$(dirname "$0")" && pwd); w=$(mktemp -d)
export LIGHTSOUT_STATE="$w/state" LIGHTSOUT_LIB="$w/lib" PATH="$w/bin:$PATH"
mkdir -p "$w/bin" "$w/lib"
printf '#!/bin/sh\nfor a; do last=$a; done\ncase " $FAKE_RUNNING " in *" $last "*) exit 0 ;; esac\nexit 1\n' > "$w/bin/pgrep"
printf '#!/bin/sh\necho "$1" >> "%s/ran"\n' "$w" > "$w/lib/switch"
chmod +x "$w/bin/pgrep" "$w/lib/switch"
ok=1
case_() {  # $1 = files in the state folder, $2 = running, $3 = expected switch calls, $4 = what it shows
  rm -rf "$LIGHTSOUT_STATE" "$w/ran"; mkdir -p "$LIGHTSOUT_STATE"
  for f in $1; do touch "$LIGHTSOUT_STATE/$f"; done
  FAKE_RUNNING="$2" sh "$here/../mac/guard"
  got=$(cat "$w/ran" 2>/dev/null | xargs)
  if [ "$got" = "$3" ]; then echo "PASS: $4"; else echo "FAIL: $4 (switch ran: '$got', expected '$3')"; ok=0; fi
}
case_ "" "" "" "nothing changed: nothing to put back"
case_ "active" "lightsout-app" "" "the app or its background copy runs: left alone"
case_ "active" "$LIGHTSOUT_LIB/logout-later" "" "in the middle of a switch: left alone"
case_ "active" "" "in sleep" "Lights Out stopped without Lights On: everything put back"
case_ "awake" "" "in sleep" "the app crashed while keeping the Mac awake: sleep put back"
[ "$ok" = 1 ]
