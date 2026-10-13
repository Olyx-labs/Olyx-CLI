"""Release archive contracts, including GNU binary checksum markers on Windows."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile


class ArchiveVerification(unittest.TestCase):
    def verify(self, *, marker="", corrupt=False, filename=None, extra=None, duplicate=False):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "olyx-v0.2.0-x86_64-pc-windows-msvc.zip"
            root = archive.stem
            with zipfile.ZipFile(archive, "w") as bundle:
                for name in ["olyx.exe", "LICENSE", "NOTICE", "README.md", "README.txt", "examples/olyx.json", "examples/request.json", "docs/storage.md"]:
                    bundle.writestr(f"{root}/{name}", "release-fixture")
                if extra:
                    bundle.writestr(f"{root}/{extra}", "unexpected")
                if duplicate:
                    with warnings.catch_warnings():
                        warnings.simplefilter("ignore", UserWarning)
                        bundle.writestr(f"{root}/olyx.exe", "duplicate")
            digest = "0" * 64 if corrupt else hashlib.sha256(archive.read_bytes()).hexdigest()
            Path(str(archive) + ".sha256").write_text(f"{digest}  {marker}{filename or archive.name}\n", encoding="ascii")
            return subprocess.run([sys.executable, str(Path(__file__).with_name("validate-archive.py")), str(archive)], capture_output=True).returncode

    def test_text_checksum(self):
        self.assertEqual(self.verify(), 0)

    def test_windows_binary_checksum(self):
        self.assertEqual(self.verify(marker="*"), 0)

    def test_corrupt_archive(self):
        self.assertNotEqual(self.verify(corrupt=True), 0)

    def test_checksum_must_not_include_a_path(self):
        self.assertNotEqual(self.verify(filename="../archive.zip"), 0)

    def test_private_files_are_rejected(self):
        self.assertNotEqual(self.verify(extra=".env"), 0)

    def test_duplicate_members_are_rejected(self):
        self.assertNotEqual(self.verify(duplicate=True), 0)


if __name__ == "__main__":
    unittest.main()
