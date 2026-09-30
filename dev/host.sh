#!/bin/sh
# Runs on the host: the installer served at http://127.0.0.1:7790/, against
# an installerd that only pretends to install (--dry-run), for working on the
# page with no VM. Nothing here is GXWI's: there is no overlay and no logon,
# only the page and the conversation behind it.
#
#     dev/host.sh            # until interrupted
#     node dev/browser/intro.mjs http://127.0.0.1:7790/
#
# The machine it pretends to be is dev/desktop.json: a desktop with Windows on
# one disk, Peios on another, a data disk, a camera card and the stick it
# booted from. MACHINE names another description, and MACHINE=this has
# installerd probe the machine it is run on, where it can see the disks and
# their partitions but not into them.
#
#     MACHINE=dev/no-disks.json dev/host.sh
#
# FAIL_AT names a phase for every pretended job to fail part way through
# (partition, format, copy, boot), to look at an installation that goes wrong.
#
#     FAIL_AT=copy dev/host.sh
#
# installerd comes from ../installer, built here.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
installerd=../installer/target/x86_64-unknown-linux-musl/debug/installerd
(cd ../installer && cargo +1.98.1 build -p installerd)
cargo +1.98.1 build
run=$(mktemp -d)
trap 'kill "$daemon" 2>/dev/null; rm -rf "$run"' EXIT INT TERM
machine=${MACHINE:-dev/desktop.json}
set -- "${1:-127.0.0.1:7790}" --dry-run --socket "$run/installerd.sock"
[ "$machine" = this ] || set -- "$@" --inventory "$machine"
[ -z "${FAIL_AT:-}" ] || set -- "$@" --fail-at "$FAIL_AT"
address=$1
shift
"$installerd" "$@" &
daemon=$!
target/debug/installer-gxwi --socket "$run/installerd.sock" --listen "$address"
