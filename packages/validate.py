"""Check installer payloads without starting services or applying registry seeds."""

import gzip
import os
import re
import subprocess
from pathlib import Path

BINARIES = (
    "installerd",
    "install-tui",
    "msip-drive",
    "oobed",
    "oobe-tui",
    "installer-gxwi",
    "oobe-gxwi",
)
CORE = ("installerd", "install-tui", "msip-drive", "oobed", "oobe-tui")


def require(condition: bool, message: str) -> None:
    """Keep package gates effective when Python assertions are disabled."""
    if not condition:
        raise ValueError(message)


def readelf(path: Path, option: str) -> str:
    """Read one ELF view, tolerating binary CRC bytes in the debug-link section."""
    return subprocess.check_output(
        ["readelf", option, str(path)],
        text=True,
        errors="replace",
        env=os.environ | {"LC_ALL": "C"},
    )


def build_id(notes: str, path: Path) -> str:
    """Extract the single GNU build identity required by split debugging."""
    matches = re.findall(r"Build ID: ([0-9a-f]+)", notes)
    require(len(matches) == 1 and len(matches[0]) >= 3, f"{path}: missing or ambiguous build ID")
    return matches[0]


def validate_elf(stage: Path, binary: str, *, native: bool) -> None:
    """Require target-specific hardening and matching split debugging information."""
    elf = stage / ("usr/sbin/" if binary in ("installerd", "oobed") else "usr/bin/") / binary
    require(elf.is_file() and os.access(elf, os.X_OK), f"{elf}: missing executable")
    header, program, dynamic = (readelf(elf, option) for option in ("-h", "-lW", "-dW"))
    notes, sections = readelf(elf, "-n"), readelf(elf, "-SW")
    require(bool(re.search(r"Type:\s+DYN\b", header)), f"{elf}: not position independent")
    require("GNU_RELRO" in program and "BIND_NOW" in dynamic, f"{elf}: missing full RELRO")
    stack = [line for line in program.splitlines() if "GNU_STACK" in line]
    require(len(stack) == 1 and bool(re.search(r"\sRW\s", stack[0])), f"{elf}: executable stack")
    require(bool(re.search(r"FLAGS_1.*\bPIE\b", dynamic)), f"{elf}: missing PIE flag")
    if binary in CORE:
        require(
            "INTERP" not in program and "NEEDED" not in dynamic, f"{elf}: CLI program is dynamic"
        )
    else:
        require(
            "Shared library: [libpeios.so.0]" in dynamic, f"{elf}: missing libpeios ABI linkage"
        )
    require(not re.search(r"\((RPATH|RUNPATH|TEXTREL)\)", dynamic), f"{elf}: unsafe dynamic tag")
    require(".debug_info" not in sections, f"{elf}: unsplit debug information")
    if native:
        # Preserve the existing static-musl exemption: Rust uses CET instrumentation,
        # but musl does not enable shadow stacks at startup (tracked in PEI-1159).
        if binary not in CORE:
            require("IBT" in notes and "SHSTK" in notes, f"{elf}: missing native CET properties")
        require(bool(re.search(r"\(RELR\)", dynamic)), f"{elf}: missing native packed relocations")
    identity = build_id(notes, elf)
    component = binary
    debug = stage / f"debug/{component}/usr/lib/debug/.build-id/{identity[:2]}/{identity[2:]}.debug"
    require(debug.is_file() and debug.stat().st_size > 0, f"{elf}: missing split debug file")
    require(".debug_info" in readelf(debug, "-SW"), f"{debug}: missing debug information")
    require(build_id(readelf(debug, "-n"), debug) == identity, f"{debug}: build ID mismatch")
    require(debug.name in readelf(elf, "--string-dump=.gnu_debuglink"), f"{elf}: wrong debug link")
    forbidden = {str(Path.cwd().resolve()), str(Path.cwd().resolve()).split("/execroot/")[0]}
    for path in (elf, debug):
        contents = path.read_bytes()
        for prefix in forbidden:
            require(len(prefix) <= 1 or prefix.encode() not in contents, f"{path}: leaked {prefix}")


def validate_data(stage: Path) -> None:
    """Verify manuals and inert registry seeds against the declared source inputs."""
    for name in BINARIES:
        section = "8" if name in ("installerd", "oobed") else "1"
        manual = stage / f"usr/share/man/man{section}/{name}.{section}.gz"
        with gzip.open(manual, "rt", encoding="utf-8") as stream:
            require(
                bool(re.search(r"^\.TH ", stream.read(), re.MULTILINE)),
                f"missing heading: {manual}",
            )
    expected = {
        path.name: path
        for directory in (Path("registry.d"), Path("installer-gxwi/registry.d"))
        for path in directory.glob("*.reg")
    }
    seeds = stage / "usr/share/regim"
    require({path.name for path in seeds.iterdir()} == set(expected), "incomplete registry seeds")
    for name, original in expected.items():
        require((seeds / name).read_bytes() == original.read_bytes(), f"changed seed: {name}")
    require(not (stage / "var").exists(), "package unexpectedly contains mutable state")


def validate(stage: Path, *, native: bool, sdk: Path) -> None:
    """Check installed payloads without starting services, changing disks or applying seeds."""
    require(sdk.is_dir(), "missing declared SDK")
    for binary in BINARIES:
        validate_elf(stage, binary, native=native)
    validate_data(stage)
