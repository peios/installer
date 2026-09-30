#!/bin/sh
# Runs on the host: first-boot setup served at http://127.0.0.1:7791/ by
# oobe-gxwi, against an oobed that only pretends (--dry-run), for working on
# its pages with no VM. dev/host.sh is the same for the installer.
#
#     dev/host-oobe.sh       # until interrupted
#     node dev/browser/welcome.mjs
#     node dev/browser/network.mjs
#
# A pretended oobed makes no account and sets no overlay: it is pointed at by
# hand, as here. oobed comes from ../installer, built here.
#
# The network it pretends the machine has is what NET_STATUS holds, as
# `net status` would print it (dev/net-status.txt by default). It is read
# again each time the network page checks, so editing it and pressing Check
# again is a cable plugged in or pulled.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
oobed=../installer/target/x86_64-unknown-linux-musl/debug/oobed
(cd ../installer && cargo +1.98.1 build -p oobed)
cargo +1.98.1 build
run=$(mktemp -d)
trap 'kill "$daemon" 2>/dev/null; rm -rf "$run"' EXIT INT TERM
"$oobed" --dry-run --socket "$run/oobed.sock" --net-status "$(realpath "${NET_STATUS:-dev/net-status.txt}")" &
daemon=$!
target/debug/oobe-gxwi --socket "$run/oobed.sock" --listen "${1:-127.0.0.1:7791}"
