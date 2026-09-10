#!/bin/sh
# Drive oobed's whole flow with known values and then ask lpsd what it
# got. An account created through the real form would not authenticate,
# and this is the half of the path a unit test cannot reach: what the
# flow hands to `lps`.
export REG_ASSUME_YES=1
OUT=/share/oobe-flow.txt
say() { echo "$@" >> "$OUT"; }
: > "$OUT"

/sbin/oobed --socket /run/oobe-probe.sock >> "$OUT" 2>&1 &
sleep 2

say "== drive the form =="
msip-drive --socket /run/oobe-probe.sock --kind oobe \
  --press nav.next \
  --press nav.next \
  --set account.name=probe2 \
  --set account.password=peiospw \
  --set account.confirm=peiospw \
  --press nav.next \
  --set hostname=workshop \
  --press nav.finish >> "$OUT" 2>&1
say "(msip-drive exit $?)"

sleep 2
say ""
say "== what lpsd ended up with =="
lps show probe2 >> "$OUT" 2>&1
say "(exit $?)"

say ""
say "== and the hostname =="
reg get 'Machine\System\Network' Hostname >> "$OUT" 2>&1
sync
