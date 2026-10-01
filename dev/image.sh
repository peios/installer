#!/bin/sh
# Runs on the host: build an Experimental medium that carries this checkout's
# installer and first-boot setup, for trying them end to end in the dev VM,
# from the installer to the first page of setup on the machine installed.
#
#     dev/image.sh                        # target/image/, a few minutes
#     IMAGE=target/image dev/boot.sh      # the dev VM, booted from it
#
# It builds, as releases, installer-gxwi and oobe-gxwi here, installerd and
# oobed in ../installer, gxwid and gxwi-server in ../gxwi, fenestra in
# ../fenestra and fenesh in ../fenesh, and layers dev/image.toml over
# ../dist/release's spec, which puts them in the image (see there). Packages
# come from ../pkgs/_repo2_ as for any medium.
set -eu
cd "$(dirname "$0")/.."
. dev/env.sh
here=$PWD
(cd ../installer && cargo +1.98.1 build --release -p installerd -p oobed)
cargo +1.98.1 build --release
(cd ../gxwi && cargo build --release -p gxwi-server --target x86_64-unknown-linux-musl && cargo build --release -p gxwid)
(cd ../fenestra && . dev/env.sh && cargo build --release)
(cd ../fenesh && . dev/env.sh && cargo build --release)
(cd ../peiso && go build -o peiso .)
cd ../dist/release
../../peiso/peiso iso experimental.toml dev.toml "$here/dev/image.toml" --out "$here/target/image" --overwrite
