#!/bin/sh
# Serve the frontend against a dry-run daemon; no installation is performed.
# PEIOS_SDK supplies host development libraries. Optional first argument is the
# listen address (default 127.0.0.1:7790). Run from any working directory.
set -eu
cd "$(dirname "$0")/../.."
# shellcheck source=installer-gxwi/dev/env.sh
. installer-gxwi/dev/env.sh
daemon_binary=$(build_binary //installerd)
frontend_binary=$(build_binary //installer-gxwi:installer-gxwi)
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
# MACHINE=this permits read-only host inventory probing; all jobs remain dry-run.
machine=${MACHINE:-installer-gxwi/dev/desktop.json}
set -- "${1:-127.0.0.1:7790}" --dry-run --socket "$run/installerd.sock"
[ "$machine" = this ] || set -- "$@" --inventory "$machine"
[ -z "${FAIL_AT:-}" ] || set -- "$@" --fail-at "$FAIL_AT"
address=$1
shift
"$daemon_binary" "$@" &
daemon=$!
"$frontend_binary" --socket "$run/installerd.sock" --listen "$address"
