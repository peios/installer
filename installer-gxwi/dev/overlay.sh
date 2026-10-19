#!/bin/sh
# Runs on the host: make the installer GXWI's overlay in the dev VM, or take
# it away again. With it on, http://127.0.0.1:7780/ is the installer, with no
# logon; with it off, it is the logon page and the desktop as before.
#
#     dev/overlay.sh on
#     dev/overlay.sh off
#
# It runs as peios, the live image's account, which has no credential and is
# an administrator: an overlay runs only as an account with no credential,
# and installerd's socket admits administrators. GXWI applies the change as
# it is made; nothing is restarted.
set -eu
cd "$(dirname "$0")/.."
guest=../gxwi/dev/guest.sh
key="'Machine\\Software\\GXWI'"
case "${1:-}" in
on)
    "$guest" "reg set $key OverlayUsername sz:peios && reg set $key OverlaySession sz:/tmp/installer-gxwi"
    ;;
off)
    "$guest" "REG_ASSUME_YES=1 reg del --yes $key OverlaySession; REG_ASSUME_YES=1 reg del --yes $key OverlayUsername"
    ;;
*)
    echo "usage: dev/overlay.sh on|off" >&2
    exit 2
    ;;
esac
