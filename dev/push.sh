#!/bin/sh
# Runs on the host: build the installer and put it in the share of the VM
# that ../gxwi/dev/boot.sh started. GXWI's dev service copies it to
# /tmp/installer-gxwi in the guest, and its dev loop restarts on a new one.
# dev/overlay.sh makes it the overlay.
#
# The rename makes the new binary appear whole, never half-written. The
# installer links libpeios, so it is an ordinary glibc build and uses the
# libpeios the guest image ships. The page is inside the binary.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
share=../gxwi/target/vmshare
[ -d "$share" ] || { echo "no $share: boot the VM from ../gxwi first" >&2; exit 1; }
cargo +1.98.1 build --release
cp target/release/installer-gxwi "$share/installer-gxwi.new"
mv "$share/installer-gxwi.new" "$share/installer-gxwi"
