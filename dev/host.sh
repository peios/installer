#!/bin/sh
# Runs on the host: the installer served at http://127.0.0.1:7790/, against
# an installerd that only pretends to install (--dry-run), for working on the
# page with no VM. Nothing here is GXWI's: there is no overlay and no logon,
# only the page and the conversation behind it.
#
#     dev/host.sh            # until interrupted
#     node dev/browser/intro.mjs http://127.0.0.1:7790/
#
# installerd comes from ../installer, built here if it is not there.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
installerd=../installer/target/x86_64-unknown-linux-musl/debug/installerd
[ -x "$installerd" ] || (cd ../installer && cargo +1.98.1 build -p installerd)
cargo +1.98.1 build
run=$(mktemp -d)
trap 'kill "$daemon" 2>/dev/null; rm -rf "$run"' EXIT INT TERM
"$installerd" --dry-run --socket "$run/installerd.sock" &
daemon=$!
target/debug/installer-gxwi --socket "$run/installerd.sock" --listen "${1:-127.0.0.1:7790}"
