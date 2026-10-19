#!/usr/bin/python3
"""Stage and validate declared Installer artifacts before writing any PPK commit."""

import gzip
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import TypedDict, cast

sys.dont_write_bytecode = True
from licences import collect, combined  # noqa: E402
from validate import validate  # noqa: E402


class ComponentInput(TypedDict):
    """Declared binary and transitive source inputs from the staging rule."""

    binary: str
    inputs: list[str]


class StageSpec(TypedDict):
    """Internal staging manifest emitted by packages/build.bzl."""

    components: dict[str, ComponentInput]
    sources: list[str]
    version: str
    sdk: str
    rustc: str
    toolchain_inputs: list[str]
    sdk_inputs: list[str]
    native: bool


DESTINATIONS = {
    "installerd": "usr/sbin/installerd",
    "install-tui": "usr/bin/install-tui",
    "msip-drive": "usr/bin/msip-drive",
    "oobed": "usr/sbin/oobed",
    "oobe-tui": "usr/bin/oobe-tui",
    "installer-gxwi": "usr/bin/installer-gxwi",
    "oobe-gxwi": "usr/bin/oobe-gxwi",
}
CORE = ("installerd", "install-tui", "msip-drive", "oobed", "oobe-tui")


def copy(source: Path, target: Path) -> None:
    """Copy a file while normalizing its mode for an installed payload."""
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(0o755 if source.stat().st_mode & 0o111 else 0o644)


def split_debug(component: str, elf: Path, output: Path) -> None:
    """Split ELF debug data and record the GNU build ID and debug link."""
    notes = subprocess.check_output(["readelf", "-n", str(elf)], text=True)
    match = re.search(r"Build ID: ([0-9a-f]+)", notes)
    if not match:
        raise ValueError(f"missing build ID: {elf}")
    build_id = match.group(1)
    debug = (
        output / f"debug/{component}/usr/lib/debug/.build-id/{build_id[:2]}/{build_id[2:]}.debug"
    )
    debug.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["objcopy", "--only-keep-debug", elf, debug], check=True)
    subprocess.run(["strip", "--strip-unneeded", elf], check=True)
    subprocess.run(["objcopy", f"--add-gnu-debuglink={debug}", elf], check=True)


def source_files(inputs: list[Path]) -> list[Path]:
    """Resolve declared source files and generated bindings, rejecting missing inputs."""
    files = set()
    for path in inputs:
        if path.is_dir():
            # Generated bindings and build-script source products, not compiled archives.
            files.update(
                p for p in path.rglob("*") if p.is_file() and p.suffix in {".rs", ".h", ".c"}
            )
        elif path.is_file():
            files.add(path)
        else:
            raise ValueError(f"missing declared input: {path}")
    return sorted(files)


def stage_data(sources: list[Path], output: Path, version: str) -> None:
    """Install compressed manuals and inert registry seeds."""
    for source in sources:
        if source.parent == Path("docs/man"):
            section = source.suffix[1:]
            target = output / f"usr/share/man/man{section}/{source.name}.gz"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(gzip.compress(source.read_bytes(), compresslevel=9, mtime=0))
        elif source.parent in (Path("registry.d"), Path("installer-gxwi/registry.d")):
            copy(source, output / "usr/share/regim" / source.name)


def check_versions(sources: list[Path], version: str) -> None:
    """Reject omitted manifests or member versions hidden by Bazel's shared version."""
    manifest = Path("Cargo.toml")
    if manifest not in sources:
        raise ValueError("missing package manifest input")
    workspace = tomllib.loads(manifest.read_text(encoding="utf-8"))["workspace"]
    if workspace["package"]["version"] != version:
        raise ValueError("package version differs from Bazel")
    for member in workspace["members"]:
        path = Path(member) / "Cargo.toml"
        if path not in sources:
            raise ValueError(f"missing workspace member manifest: {path}")
        package = tomllib.loads(path.read_text(encoding="utf-8"))["package"]
        if package.get("version") not in (version, {"workspace": True}):
            raise ValueError(f"workspace member version differs from Bazel: {path}")


def _stage_components(
    components: dict[str, ComponentInput], output: Path, debug_inputs: set[Path], sdk: Path
) -> dict[str, str]:
    """Split each program and retain its actual dependency and debug-source closure."""
    if set(components) != set(DESTINATIONS):
        raise ValueError("incomplete or unexpected installer binary set")
    groups = {"peios-installer": set(debug_inputs), "installer-gxwi": set(debug_inputs)}
    expressions = {}
    for name, component in components.items():
        elf = output / DESTINATIONS[name]
        copy(Path(component["binary"]), elf)
        split_debug(name, elf, output)
        inputs = set(source_files([Path(path) for path in component["inputs"]])) | debug_inputs
        expressions[name] = _collect_notices(inputs, output, name, graphical=name not in CORE)
        groups["peios-installer" if name in CORE else "installer-gxwi"].update(inputs)
    for group, inputs in groups.items():
        name = group + "-debugsource"
        expressions[name] = _collect_notices(
            inputs, output, name, graphical=group == "installer-gxwi"
        )
        _stage_debug_sources(inputs, output / "debugsource" / group, sdk)
        debug_inputs.update(inputs)
    return expressions


def _collect_notices(inputs: set[Path], output: Path, name: str, *, graphical: bool) -> str:
    directory = output / "licenses" / name
    expression = collect(sorted(inputs), directory)
    if graphical:
        return combined({expression, "OFL-1.1"})
    copy(Path("third-party/musl-COPYRIGHT"), directory / "musl-COPYRIGHT")
    return expression


def _stage_debug_sources(debug_inputs: set[Path], output: Path, sdk: Path) -> None:
    """Copy only declared sources, preserving the compiler's remapped source paths."""
    debug_root = output / "usr/src/debug/dev.peios.peios-installer"
    for source in sorted(debug_inputs):
        if source.is_relative_to(sdk) or source.suffix in {".so", ".a", ".rlib", ".o"}:
            continue
        if source.is_absolute() or ".." in source.parts:
            raise ValueError(f"non-relative declared source: {source}")
        copy(source, debug_root / source)


def _check_licences(expressions: dict[str, str]) -> None:
    for path in sorted(Path("packages/definitions").glob("*.json")):
        definition = json.loads(path.read_text(encoding="utf-8"))
        name = definition["package"]["name"].removeprefix("dev.peios.")
        component = name.removesuffix("-debuginfo")
        if component in expressions and definition["package"]["license"] != expressions[component]:
            raise ValueError(f"{path}: licence expression differs: {expressions[component]}")


def input_identity(names: list[str]) -> str:
    """Fingerprint the explicitly declared file closure without ambient cache traversal."""
    fingerprints = {}
    for name in sorted(set(names)):
        path = Path(name)
        if not path.is_file():
            raise ValueError(f"missing declared provenance input: {path}")
        with path.open("rb") as stream:
            fingerprints[name] = hashlib.file_digest(stream, "sha256").hexdigest()
    return hashlib.sha256(json.dumps(fingerprints, sort_keys=True).encode()).hexdigest()


def _write_build_record(
    spec: StageSpec, output: Path, debug_inputs: set[Path], expressions: dict[str, str]
) -> None:
    fingerprints = {
        str(path): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(debug_inputs)
        if path.is_file()
    }
    record = {
        "version": spec["version"],
        "userspace_abi": "peios-native" if spec["native"] else "debian-development",
        "release_ready": False,
        "signed": False,
        "checks": "native-hardening-and-payload" if spec["native"] else "development-payload",
        "input_digest": hashlib.sha256(
            json.dumps(fingerprints, sort_keys=True).encode("utf-8")
        ).hexdigest(),
        "licences": expressions,
        "compiler": {
            "version": subprocess.check_output(
                [spec["rustc"], "--version", "--verbose"], text=True
            ).strip(),
            "input_digest": input_identity(spec["toolchain_inputs"]),
        },
        "sdk_input_digest": input_identity(spec["sdk_inputs"]),
    }
    (output / "build.json").write_text(
        json.dumps(record, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )


def main() -> None:
    """Stage the declared build inputs and write provenance after validation succeeds."""
    spec = cast(StageSpec, json.loads(Path(sys.argv[1]).read_text(encoding="utf-8")))
    output = Path(sys.argv[2])
    output.mkdir(parents=True, exist_ok=True)
    sources = [Path(path) for path in spec["sources"]]
    check_versions(sources, spec["version"])
    debug_inputs = set(sources)
    expressions = _stage_components(spec["components"], output, debug_inputs, Path(spec["sdk"]))
    _check_licences(expressions)
    stage_data(sources, output, spec["version"])
    validate(output, native=spec["native"], sdk=Path(spec["sdk"]))
    _write_build_record(spec, output, debug_inputs, expressions)


if __name__ == "__main__":
    main()
