#!/bin/sh
# Install onto the attached blank disk with installerd driven headlessly,
# then read the seed queues off the target.
#
# What it proves:
#   1. An installer copy no longer carries the medium's own seeds
#      (lpsd-first-account, whose password ships in the image).
#   2. It does carry the ones staged for an installed machine
#      (oobe-service), promoted into the queue peinit actually drains.
#   3. Neither staging directory survives on the target.
#
# Run with dist/release/drive.py --share <dir> (the disk must be attached).
export REG_ASSUME_YES=1
OUT=/share/install-queues.txt
say() { echo "$@" >> "$OUT"; }
: > "$OUT"

say "== driving installerd =="
msip-drive --socket /run/installerd.sock --kind install \
  --press act.install \
  --set disk.target=/dev/vdb --press nav.next \
  --press act.begin >> "$OUT" 2>&1
say "(msip-drive exit $?)"

say ""
say "== the target's seed queues =="
mkdir -p /run/t
mount -o policy=deny-missing /dev/vdb2 /run/t >> "$OUT" 2>&1
say "-- autoapply.d (what peinit drains; expect oobe-service, no lpsd-first-account):"
ls /run/t/lcl/policy/autoapply.d >> "$OUT" 2>&1
say "-- autoapply.live.d (expect: no such directory):"
ls -d /run/t/lcl/policy/autoapply.live.d >> "$OUT" 2>&1
say "-- autoapply.install.d (expect: no such directory):"
ls -d /run/t/lcl/policy/autoapply.install.d >> "$OUT" 2>&1
say "-- the setup binaries the seed names:"
ls -l /run/t/usr/sbin/oobed /run/t/usr/bin/oobe-tui >> "$OUT" 2>&1
say "-- the drain script peinit runs:"
cat /run/t/lcl/policy/autorun.d/10-apply-seeds.sh >> "$OUT" 2>&1
umount /run/t >> "$OUT" 2>&1

say ""
say "== this medium's own queues, for contrast =="
ls /lcl/policy/autoapply.live.d /lcl/policy/autoapply.install.d >> "$OUT" 2>&1
sync
