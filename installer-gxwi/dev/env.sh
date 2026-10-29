#!/bin/sh
# Sourced by host build helpers after changing to the consolidated workspace
# root. pekit builds each program, and libpeios from the Peios SDK's own
# files, which the browser interfaces load.

# The output of the pekit target $1, which pekit prints, indented, on the
# line after the target's name and a count of its steps.
build_output() {
    output=$(pekit build "$1" | awk -v target="built /$1" 'found { sub(/^  /, ""); print; exit } $0 == target || index($0, target ":") == 1 { found = 1 }')
    [ -n "$output" ] && [ -e "$output" ] || {
        echo "expected one output from $1, got: $output" >&2
        return 1
    }
    realpath "$output"
}

build_binary() {
    binary=$(build_output "$1")
    [ -f "$binary" ] && [ -x "$binary" ] || {
        echo "expected one executable from $1, got: $binary" >&2
        return 1
    }
    printf '%s\n' "$binary"
}

libpeios=$(build_output libpeios_dir)
export LD_LIBRARY_PATH="$libpeios${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
