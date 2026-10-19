# Sourced from the installer-gxwi root, not run: what cargo needs to build
# against the `peios` crate from sibling checkouts. A packaged build would get
# these from pekit instead.
REPO="$(cd .. && pwd)"
export BINDGEN_EXTRA_CLANG_ARGS="-isystem $(gcc -print-file-name=include)"
export PEIOS_LIB_DIR="$REPO/libpeios/target/debug"
export PEIOS_INCLUDE="$REPO/libpeios/include"
export PKM_UAPI="$REPO/pkm/out/build/headers/usr/include"
export LD_LIBRARY_PATH="$REPO/libpeios/target/debug"
