"""Reject package regressions in version consistency and ELF deployment contracts."""

import tempfile
import unittest
from contextlib import chdir
from pathlib import Path
from unittest.mock import patch

from stage import check_versions
from validate import validate_elf


class VersionTests(unittest.TestCase):
    """Bazel package versions must agree with every Cargo workspace member."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.location = chdir(self.temporary.name)
        self.location.__enter__()
        self.addCleanup(self.location.__exit__, None, None, None)
        Path("Cargo.toml").write_text(
            '[workspace]\nmembers = ["engine", "frontend"]\n'
            '[workspace.package]\nversion = "0.1.13"\n'
        )
        self.sources = [Path("Cargo.toml")]
        for name in ("engine", "frontend"):
            manifest = Path(name) / "Cargo.toml"
            manifest.parent.mkdir()
            manifest.write_text(f'[package]\nname = "{name}"\nversion.workspace = true\n')
            self.sources.append(manifest)

    def test_inherited_member_versions_are_accepted(self) -> None:
        check_versions(self.sources, "0.1.13")

    def test_matching_explicit_member_version_is_accepted(self) -> None:
        Path("frontend/Cargo.toml").write_text('[package]\nname = "frontend"\nversion = "0.1.13"\n')
        check_versions(self.sources, "0.1.13")

    def test_conflicting_member_version_is_rejected(self) -> None:
        Path("frontend/Cargo.toml").write_text('[package]\nname = "frontend"\nversion = "0.2.0"\n')
        with self.assertRaises(ValueError):
            check_versions(self.sources, "0.1.13")

    def test_undeclared_member_manifest_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            check_versions(self.sources[:-1], "0.1.13")

    def test_missing_member_manifest_is_rejected(self) -> None:
        Path("frontend/Cargo.toml").unlink()
        with self.assertRaises((ValueError, FileNotFoundError)):
            check_versions(self.sources, "0.1.13")

    def test_workspace_and_bazel_versions_must_match(self) -> None:
        with self.assertRaises(ValueError):
            check_versions(self.sources, "0.2.0")


class ElfContractTests(unittest.TestCase):
    """Mutate readelf views to test independent package acceptance requirements.

    Real ELF inspection runs during every package-stage action. These fixtures
    exercise rejection paths that a correctly linked payload does not reach.
    """

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.stage = Path(self.temporary.name)
        self.binary = "installerd"
        self.program = "  GNU_RELRO 0x0000\n  GNU_STACK 0x0000 RW 0x10\n"
        self.dynamic = " (FLAGS) BIND_NOW\n (FLAGS_1) Flags: NOW PIE\n (RELR) 0x1000\n"
        self.properties = ""
        self.debug_identity = "aabbccdd"
        self.debug_link = "bbccdd.debug"
        self.debug_sections = " .debug_info PROGBITS\n"
        self.sections = " .text PROGBITS\n .gnu_debuglink PROGBITS\n"
        self.patch = patch("validate.readelf", side_effect=self.readelf)
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def readelf(self, path: Path, option: str) -> str:
        """Return standard readelf fields for one valid or deliberately altered file."""
        debug = path.suffix == ".debug"
        if option == "-h":
            return "  Type: DYN (Position-Independent Executable file)\n"
        if option == "-lW":
            return self.program
        if option == "-dW":
            return self.dynamic
        if option == "-n":
            identity = self.debug_identity if debug else "aabbccdd"
            return f"    Build ID: {identity}\n{self.properties}"
        if option == "-SW":
            return self.debug_sections if debug else self.sections
        if option == "--string-dump=.gnu_debuglink":
            return self.debug_link
        raise AssertionError(f"Unexpected readelf operation: {option}")

    def prepare(self, *, graphical: bool = False) -> None:
        """Create payload entries whose ELF views are supplied by the fixture."""
        if graphical:
            self.binary = "installer-gxwi"
            self.program += "  INTERP 0x1000 R 0x1\n"
            self.dynamic += " (NEEDED) Shared library: [libpeios.so.0]\n"
            self.properties = "x86 feature: IBT, SHSTK\n"
        directory = "usr/bin" if graphical else "usr/sbin"
        elf = self.stage / directory / self.binary
        elf.parent.mkdir(parents=True)
        elf.write_bytes(b"ELF fixture")
        elf.chmod(0o755)
        debug = self.stage / f"debug/{self.binary}/usr/lib/debug/.build-id/aa/bbccdd.debug"
        debug.parent.mkdir(parents=True)
        debug.write_bytes(b"Split debug fixture")

    def test_static_musl_exemption_does_not_require_cet_notes(self) -> None:
        self.prepare()
        validate_elf(self.stage, self.binary, native=True)

    def test_core_program_cannot_acquire_a_dynamic_dependency(self) -> None:
        self.prepare()
        self.dynamic += " (NEEDED) Shared library: [libc.so.6]\n"
        with self.assertRaisesRegex(ValueError, "CLI program is dynamic"):
            validate_elf(self.stage, self.binary, native=True)

    def test_native_graphical_program_requires_both_cet_properties(self) -> None:
        self.prepare(graphical=True)
        validate_elf(self.stage, self.binary, native=True)
        self.properties = "x86 feature: IBT\n"
        with self.assertRaisesRegex(ValueError, "CET"):
            validate_elf(self.stage, self.binary, native=True)

    def test_graphical_program_requires_the_libpeios_abi(self) -> None:
        self.prepare(graphical=True)
        self.dynamic = self.dynamic.replace("libpeios.so.0", "libpeios.so.1")
        with self.assertRaisesRegex(ValueError, "libpeios ABI"):
            validate_elf(self.stage, self.binary, native=False)

    def test_native_program_requires_packed_relocations(self) -> None:
        self.prepare()
        self.dynamic = self.dynamic.replace(" (RELR) 0x1000\n", "")
        with self.assertRaisesRegex(ValueError, "packed relocations"):
            validate_elf(self.stage, self.binary, native=True)

    def test_executable_stack_is_rejected(self) -> None:
        self.prepare()
        self.program = self.program.replace(" RW ", " RWE ")
        with self.assertRaisesRegex(ValueError, "executable stack"):
            validate_elf(self.stage, self.binary, native=False)

    def test_split_debug_identity_must_match(self) -> None:
        self.prepare()
        self.debug_identity = "11223344"
        with self.assertRaisesRegex(ValueError, "build ID mismatch"):
            validate_elf(self.stage, self.binary, native=False)

    def test_runtime_path_is_rejected(self) -> None:
        self.prepare()
        self.dynamic += " (RUNPATH) Library runpath: [/build/sdk/lib]\n"
        with self.assertRaisesRegex(ValueError, "unsafe dynamic tag"):
            validate_elf(self.stage, self.binary, native=False)


if __name__ == "__main__":
    unittest.main()
