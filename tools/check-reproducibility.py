#!/usr/bin/env python3
"""Compare all unstripped package programs from two independent native Bazel builds."""

import argparse
import hashlib
import json
from pathlib import Path

PROGRAMS = {
    "installerd",
    "install-tui",
    "msip-drive",
    "oobed",
    "oobe-tui",
    "installer-gxwi",
    "oobe-gxwi",
}


def identities(root: Path) -> tuple[str, dict[str, str]]:
    """Read the staging action's declared binaries, not stripped package copies."""
    manifests = list(root.glob("bazel-out/*/bin/packages/stage.inputs.json"))
    if len(manifests) != 1:
        raise ValueError(f"expected exactly one package staging manifest under {root}")
    spec = json.loads(manifests[0].read_text(encoding="utf-8"))
    if not spec["native"] or set(spec["components"]) != PROGRAMS:
        raise ValueError("expected the complete native installer program family")
    result = {}
    for name, component in spec["components"].items():
        path = root / component["binary"]
        with path.open("rb") as stream:
            result[name] = hashlib.file_digest(stream, "sha256").hexdigest()
    return spec["version"], result


def compare(first: Path, second: Path) -> dict[str, object]:
    """Reject reused build roots, version differences and any changed executable byte."""
    if first.resolve() == second.resolve():
        raise ValueError("reproducibility requires two separate build roots")
    first_version, first_hashes = identities(first)
    second_version, second_hashes = identities(second)
    if first_version != second_version:
        raise ValueError("build versions differ")
    changed = [name for name in sorted(PROGRAMS) if first_hashes[name] != second_hashes[name]]
    if changed:
        raise ValueError("unstripped programs differ: " + ", ".join(changed))
    return {"version": first_version, "reproducible": True, "programs": first_hashes}


def main() -> None:
    """Emit a machine-readable comparison after the caller has built both source views."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("first", type=Path, help="first work/output/execroot/_main directory")
    parser.add_argument("second", type=Path, help="second work/output/execroot/_main directory")
    args = parser.parse_args()
    print(json.dumps(compare(args.first, args.second), sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
