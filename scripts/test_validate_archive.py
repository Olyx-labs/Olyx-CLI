"""Release tarball contracts and portable SHA-256 checksum validation."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import io
import tarfile


class ArchiveVerification(unittest.TestCase):
    def verify(self, *, marker="", corrupt=False, filename=None, extra=None, duplicate=False):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "olyx-v0.2.0-aarch64-apple-darwin.tar.gz"
            root = archive.name[:-7]
            with tarfile.open(archive, "w:gz") as bundle:
                def add(name, payload):
                    data = payload.encode()
                    entry = tarfile.TarInfo(f"{root}/{name}")
                    entry.size = len(data)
                    bundle.addfile(entry, io.BytesIO(data))
                for name in ["oy", "LICENSE", "NOTICE", "README.md", "README.txt", "CHANGELOG.md", "SECURITY.md", "examples/olyx.json", "examples/request.json", "examples/providers.json", "examples/fallbacks.json", "examples/rag.json", "examples/rag.py", "docs/storage.md", "docs/providers.md", "docs/deployment.md", "docs/rag.md", "docs/diagnostics.md", "examples/ollama.json", "examples/plugins/echo.py", "docs/plugins.md", "docs/provider-testing.md", "scripts/test-plugin", "scripts/certify-providers.py"]:
                    add(name, "release-fixture")
                if extra:
                    add(extra, "unexpected")
                if duplicate:
                    add("oy", "duplicate")
            digest = "0" * 64 if corrupt else hashlib.sha256(archive.read_bytes()).hexdigest()
            Path(str(archive) + ".sha256").write_text(f"{digest}  {marker}{filename or archive.name}\n", encoding="ascii")
            return subprocess.run([sys.executable, str(Path(__file__).with_name("validate-archive.py")), str(archive)], capture_output=True).returncode

    def test_text_checksum(self):
        self.assertEqual(self.verify(), 0)

    def test_binary_checksum_marker(self):
        self.assertEqual(self.verify(marker="*"), 0)

    def test_corrupt_archive(self):
        self.assertNotEqual(self.verify(corrupt=True), 0)

    def test_checksum_must_not_include_a_path(self):
        self.assertNotEqual(self.verify(filename="../archive.tar.gz"), 0)

    def test_private_files_are_rejected(self):
        self.assertNotEqual(self.verify(extra=".env"), 0)

    def test_duplicate_members_are_rejected(self):
        self.assertNotEqual(self.verify(duplicate=True), 0)


if __name__ == "__main__":
    unittest.main()
