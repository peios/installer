#!/bin/sh
# oobed creates the first account by piping one line to `lps add`. An
# account it made would not authenticate, so this asks whether the
# account is made at all, and what it looks like.
export REG_ASSUME_YES=1
OUT=/share/lps-add.txt
say() { echo "$@" >> "$OUT"; }
: > "$OUT"

say "== exactly what oobed runs =="
printf 'peiospw\n' | lps add probe --group Administrators >> "$OUT" 2>&1
say "(exit $?)"

say ""
say "== is it there =="
lps show probe >> "$OUT" 2>&1
say "(exit $?)"

say ""
say "== for contrast, the account the live image ships =="
lps show peios >> "$OUT" 2>&1

say ""
say "== everything the store holds =="
lps list >> "$OUT" 2>&1
sync
