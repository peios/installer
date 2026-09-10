#!/bin/sh
# Reproduce first-boot setup's bind on a live medium, where there is a
# shell to ask questions from. The installed system has neither.
export REG_ASSUME_YES=1
OUT=/share/oobe-probe.txt
say() { echo "$@" >> "$OUT"; }
: > "$OUT"

say "== the entropy source a conversation id comes from =="
ls -l /dev/urandom /dev/random >> "$OUT" 2>&1
say "-- read 16 bytes of it:"
dd if=/dev/urandom of=/run/entropy.bin bs=16 count=1 >> "$OUT" 2>&1
ls -l /run/entropy.bin >> "$OUT" 2>&1
say "-- as this shell's principal:"
whoami >> "$OUT" 2>&1

say ""
say "== oobed, run by hand =="
oobed --socket /run/oobe-probe.sock >> "$OUT" 2>&1 &
sleep 2
say "-- socket:"
ls -l /run/oobe-probe.sock >> "$OUT" 2>&1
say "-- drive it:"
msip-drive --socket /run/oobe-probe.sock --kind oobe --press nav.next >> "$OUT" 2>&1
say "(msip-drive exit $?)"

say ""
say "== the same as SYSTEM would see it: the installed seed's services =="
say "-- is /sbin/oobed there and executable:"
ls -l /sbin/oobed /bin/oobe-tui >> "$OUT" 2>&1
say "-- the seed the installer will promote:"
cat /lcl/policy/autoapply.install.d/oobe-service.reg >> "$OUT" 2>&1
sync
