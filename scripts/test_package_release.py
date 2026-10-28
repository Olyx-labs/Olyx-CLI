"""Exercise real archive construction, validation, and checksum rejection."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("packager", Path(__file__).with_name("package-release.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)

class Packaging(unittest.TestCase):
    def test_all_platform_archives_match_release_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(b"prebuilt executable fixture")
            for target in sorted(packager.TARGETS):
                archive = packager.package(binary, "v0.2.0", target, root / "dist")
                checksum = Path(str(archive) + ".sha256").read_bytes()
                self.assertTrue(checksum.endswith(b"\n"))
                self.assertNotIn(b"\r", checksum)
                subprocess.run([sys.executable, str(Path(__file__).with_name("validate-archive.py")), str(archive)], check=True, capture_output=True)
    def test_invalid_versions_and_targets_are_rejected(self):
        for version, target in [("../../bad", "aarch64-apple-darwin"), ("v0.2.0", "unsupported")]:
            with self.assertRaises(ValueError):
                packager.package(Path(__file__), version, target, Path("unused"))

if __name__ == "__main__":
    unittest.main()
