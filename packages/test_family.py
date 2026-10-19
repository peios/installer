"""Inspect canonical installer manifests and package boundaries from the actual build."""

import json
import tomllib
import unittest
from pathlib import Path
from typing import Any, ClassVar

CORE = ("installerd", "install-tui", "msip-drive", "oobed", "oobe-tui")
GUI = ("installer-gxwi", "oobe-gxwi")
BINARIES = {
    name: "usr/" + ("sbin/" if name in ("installerd", "oobed") else "bin/") + name
    for name in CORE + GUI
}
NAMES = (
    set(CORE + GUI)
    | {name + "-debuginfo" for name in CORE + GUI}
    | {
        "peios-installer",
        "oobe",
        "peios-installer-debugsource",
        "installer-gxwi-debugsource",
        "peios-installer-source",
    }
)


class FamilyTests(unittest.TestCase):
    """Hold source/version, independent payload and inert seed contracts."""

    index: ClassVar[dict[str, Any]]
    packages: ClassVar[dict[str, Any]]
    manifests: ClassVar[dict[str, Any]]

    @classmethod
    def setUpClass(cls) -> None:
        root = Path("packages.store")
        cls.index = json.loads((root / "index.json").read_text())
        cls.packages = {p["name"].removeprefix("dev.peios."): p for p in cls.index["packages"]}
        cls.manifests = {
            name: tomllib.loads((root / p["manifest"]).read_text())["files"]
            for name, p in cls.packages.items()
        }

    def test_complete_unsigned_family(self) -> None:
        self.assertEqual(set(self.packages), NAMES)
        self.assertEqual(self.index["package_count"], 19)
        self.assertEqual({p["version"] for p in self.packages.values()}, {"0.1.13-1"})
        self.assertFalse(self.index["release_ready"])
        self.assertEqual(self.index["trust"], "development")
        for item in self.packages.values():
            self.assertFalse(item["release_ready"])

    def test_programs_remain_separate(self) -> None:
        for name, files in self.manifests.items():
            expected = {BINARIES[name]} if name in BINARIES else set()
            self.assertEqual(set(BINARIES.values()) & files.keys(), expected)
            self.assertFalse(any(path.startswith("var/") for path in files))
        for name in CORE + GUI:
            metadata = self.packages[name]["package_metadata"]
            self.assertEqual(metadata["side_effects"], ["man-db"])
            self.assertIn("usr/share/licenses/dev.peios." + name + "/LICENSE", self.manifests[name])

    def test_seeds_stay_in_their_own_packages(self) -> None:
        owners = {
            "peios-installer": "installerd-service.reg",
            "oobe": "oobe-service.reg",
            "installer-gxwi": "installer-gxwi-overlay.reg",
        }
        for name, files in self.manifests.items():
            seeds = {p for p in files if p.startswith("usr/share/regim/")}
            expected = {"usr/share/regim/" + owners[name]} if name in owners else set()
            self.assertEqual(seeds, expected)
        for name, deps in (("peios-installer", CORE[:3]), ("oobe", ("oobed", "oobe-tui"))):
            actual = self.packages[name]["package_metadata"]["dependencies"]
            for dep in deps:
                self.assertEqual(actual["dev.peios." + dep], "= 0.1.13-1")

    def test_debug_and_manual_payloads(self) -> None:
        for name in CORE + GUI:
            section = 8 if name in ("installerd", "oobed") else 1
            self.assertIn(f"usr/share/man/man{section}/{name}.{section}.gz", self.manifests[name])
            debug = self.manifests[name + "-debuginfo"]
            self.assertEqual(sum(path.endswith(".debug") for path in debug), 1)
        for name, crates in (("peios-installer", CORE), ("installer-gxwi", ("installer-gxwi",))):
            files = self.manifests[name + "-debugsource"]
            for crate in crates:
                self.assertIn(f"usr/src/debug/dev.peios.peios-installer/{crate}/src/main.rs", files)

    def test_source_is_complete_without_caches(self) -> None:
        files = self.manifests["peios-installer-source"]
        for path in (
            "Cargo.toml",
            "Cargo.lock",
            "MODULE.bazel",
            "MODULE.bazel.lock",
            "BUILD.bazel",
            "docs/building.md",
            "tools/check",
            "vendor/ppk/src/main.rs",
        ):
            self.assertIn("usr/src/installer/" + path, files)
        for crate in (*CORE, "msip-serve", "msip-tui", "installer-gxwi"):
            self.assertIn(f"usr/src/installer/{crate}/Cargo.toml", files)
            self.assertIn(f"usr/src/installer/{crate}/BUILD.bazel", files)
        forbidden = {".git", "target", "out", "node_modules", "__pycache__", ".ruff_cache"}
        for path in files:
            self.assertFalse(forbidden.intersection(Path(path).parts), path)
            self.assertNotIn("pekit", path)


if __name__ == "__main__":
    unittest.main()
