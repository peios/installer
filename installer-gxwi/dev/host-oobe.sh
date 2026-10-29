#!/bin/sh
# Serve the frontend against a dry-run daemon; no installation is performed.
# pekit builds the programs and libpeios. Optional first argument is the
# listen address (default 127.0.0.1:7791). Run from any working directory.
set -eu
cd "$(dirname "$0")/../.."
# shellcheck source=installer-gxwi/dev/env.sh
. installer-gxwi/dev/env.sh
daemon_binary=$(build_binary oobed)
frontend_binary=$(build_binary oobe_gxwi)
run=$(mktemp -d)
daemon=
cleanup() {
    if [ -n "$daemon" ]; then
        kill "$daemon" 2>/dev/null || : # The child may already have exited.
        wait "$daemon" 2>/dev/null || :
    fi
    rm -rf "$run"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
address=${1:-127.0.0.1:7791}
"$daemon_binary" --dry-run --socket "$run/oobed.sock" --net-status "$(realpath "${NET_STATUS:-installer-gxwi/dev/net-status.txt}")" &
daemon=$!
"$frontend_binary" --socket "$run/oobed.sock" --listen "$address"
