#!/bin/sh
# The exact call first-boot setup makes to name the machine. It got this
# wrong once -- `sz` and the name as two arguments, where `reg set` wants
# the type and the data as one token -- and no unit test could catch it,
# because the flow tests drive a double rather than the real `reg`.
export REG_ASSUME_YES=1
OUT=/share/reg-hostname.txt
say() { echo "$@" >> "$OUT"; }
: > "$OUT"

say "== what setup used to run =="
reg set 'Machine\System\Network' Hostname sz workshop >> "$OUT" 2>&1
say "(exit $?)"

say ""
say "== what it runs now =="
reg set 'Machine\System\Network' Hostname sz:workshop >> "$OUT" 2>&1
say "(exit $?)"

say ""
say "== read it back (expect the string workshop, not a number) =="
reg get 'Machine\System\Network' Hostname >> "$OUT" 2>&1

say ""
say "== and a name that would infer as a number without the prefix =="
reg set 'Machine\System\Network' Hostname sz:12345 >> "$OUT" 2>&1
reg get 'Machine\System\Network' Hostname >> "$OUT" 2>&1
sync
