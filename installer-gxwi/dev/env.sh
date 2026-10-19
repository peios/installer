#!/bin/sh
# Sourced by host build helpers after changing to the consolidated workspace root.
: "${PEIOS_SDK:?set PEIOS_SDK to an absolute development SDK directory}"
case "$PEIOS_SDK" in /*) ;; *)
    echo "PEIOS_SDK must be absolute" >&2
    exit 1
    ;;
esac
[ -d "$PEIOS_SDK/lib" ] || {
    echo "missing SDK libraries: $PEIOS_SDK/lib" >&2
    exit 1
}
export LD_LIBRARY_PATH="$PEIOS_SDK/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Bazel transitions may place executables outside bazel-bin's default paths.
build_binary() {
    ./bazel build "$1" >&2
    binary=$(./bazel cquery --output=files "$1")
    [ -f "$binary" ] && [ -x "$binary" ] || {
        echo "expected one executable from $1, got: $binary" >&2
        return 1
    }
    realpath "$binary"
}
