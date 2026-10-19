#!/bin/sh
# Copy the matching frontend and daemon into an explicitly chosen development VM
# share. The guest runner must implement the existing hot-reload share contract.
set -eu
: "${VM_SHARE:?set VM_SHARE to the development VM share directory}"
share=$(realpath "$VM_SHARE")
[ -d "$share" ] || {
    echo "VM_SHARE is not a directory: $share" >&2
    exit 1
}
cd "$(dirname "$0")/../.."
# shellcheck source=installer-gxwi/dev/env.sh
. installer-gxwi/dev/env.sh
for target in //installerd //installer-gxwi:installer-gxwi; do
    binary=$(build_binary "$target")
    name=${target##*:}
    name=${name##*/}
    staging=$(mktemp "$share/.$name.XXXXXX")
    trap 'rm -f "$staging"' EXIT
    cp "$binary" "$staging"
    chmod 755 "$staging"
    mv "$staging" "$share/$name"
    trap - EXIT
done
