"""Exercise the release binary comparison using disposable declared build views."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "reproducibility", Path(__file__).with_name("check-reproducibility.py")
)
assert spec is not None and spec.loader is not None
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReproducibilityTests(unittest.TestCase):
    """A separate root and every unstripped program are required to establish equality."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.first = Path(self.temporary.name) / "first"
        self.second = Path(self.temporary.name) / "second"
        for root in (self.first, self.second):
            folder = root / "bazel-out/k8-opt/bin/packages"
            folder.mkdir(parents=True)
            components = {}
            for name in module.PROGRAMS:
                binary = root / name
                binary.write_bytes(name.encode() + b"debug data")
                components[name] = {"binary": name}
            (folder / "stage.inputs.json").write_text(
                json.dumps({"version": "0.1.13", "native": True, "components": components})
            )

    def test_identical_independent_builds(self) -> None:
        self.assertTrue(module.compare(self.first, self.second)["reproducible"])

    def test_changed_debug_data_is_a_difference(self) -> None:
        with (self.second / "oobe-gxwi").open("ab") as stream:
            stream.write(b"changed debug data")
        with self.assertRaisesRegex(ValueError, "oobe-gxwi"):
            module.compare(self.first, self.second)

    def test_reused_build_root_is_not_reproducibility(self) -> None:
        with self.assertRaisesRegex(ValueError, "separate build roots"):
            module.compare(self.first, self.first)

    def test_missing_program_fails(self) -> None:
        (self.second / "installerd").unlink()
        with self.assertRaises(FileNotFoundError):
            module.compare(self.first, self.second)


if __name__ == "__main__":
    unittest.main()
