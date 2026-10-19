#!/bin/sh
# Runs on the host: build the installer and put it in the share of the VM
# that dev/boot.sh (or ../gxwi/dev/boot.sh) started. GXWI's dev service copies
# it to /tmp/installer-gxwi in the guest, and its dev loop restarts on a new
# one. dev/overlay.sh makes it the overlay.
#
# installerd goes with it, from ../installer, and takes the place of the one
# the image ships: the two are written together, and a page that draws what
# installerd has only just learned to say needs the installerd that says it.
#
# The rename makes a new binary appear whole, never half-written. The
# installer links libpeios, so it is an ordinary glibc build and uses the
# libpeios the guest image ships; installerd is static. The page is inside
# the installer's binary.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
share=../gxwi/target/vmshare
[ -d "$share" ] || { echo "no $share: boot the VM first" >&2; exit 1; }
(cd ../installer && cargo +1.98.1 build --release -p installerd)
cp ../installer/target/x86_64-unknown-linux-musl/release/installerd "$share/installerd.new"
mv "$share/installerd.new" "$share/installerd"
cargo +1.98.1 build --release
cp target/release/installer-gxwi "$share/installer-gxwi.new"
mv "$share/installer-gxwi.new" "$share/installer-gxwi"
