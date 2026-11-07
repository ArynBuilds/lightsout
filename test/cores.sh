#!/bin/sh
# lightsout-cores runs every 5 s in Lights Out: after its start it may start no program but
# `sleep` (each one wakes the processor), and it still reads the CPU pressure. Runs as root
# (its file in /run).
set -eu
here=$(cd "$(dirname "$0")" && pwd); fake=$(mktemp -d)
for p in cat awk tr sort tail nproc systemctl; do
  real=$(command -v "$p")
  printf '#!/bin/sh\n[ -f "%s/looping" ] && echo "%s" >> "%s/started"\nexec %s "$@"\n' "$fake" "$p" "$fake" "$real" > "$fake/$p"
done
# the fake sleep: from the first loop on, programs count; three loops, then the script ends
printf '#!/bin/sh\ntouch "%s/looping"; echo x >> "%s/loops"\n[ "$(wc -l < "%s/loops")" -le 3 ]\n' "$fake" "$fake" "$fake" > "$fake/sleep"
chmod +x "$fake"/*
made=""; [ -s /run/lightsout/cores.frugal ] || { mkdir -p /run/lightsout; echo 0 > /run/lightsout/cores.frugal; made=1; }
PATH="$fake:$PATH" sh -x "$here/../os/overlay/usr/local/sbin/lightsout-cores" 2> "$fake/trace" || true
[ -n "$made" ] && rm -f /run/lightsout/cores.frugal
ok=1
if [ -s "$fake/started" ]; then echo "FAIL: the loop started: $(sort "$fake/started" | uniq -c | tr '\n' ' ')"; ok=0
else echo "PASS: the cores helper's loop starts no program but sleep"; fi
if grep -Eq '^\+ p=[0-9]+$' "$fake/trace"; then echo "PASS: it reads the CPU pressure ($(grep -E '^\+ p=[0-9]+$' "$fake/trace" | tail -n1))"
else echo "FAIL: no pressure read: $(tail -n 5 "$fake/trace" | tr '\n' ' ')"; ok=0; fi
rm -rf "$fake"; [ "$ok" = 1 ]
