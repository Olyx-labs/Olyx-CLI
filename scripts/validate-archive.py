"""Verify a release archive and its portable adjacent SHA-256 checksum."""
import hashlib
from pathlib import Path, PurePosixPath
import sys
import tarfile


def main():
    archive = Path(sys.argv[1])
    checksum = Path(str(archive) + ".sha256").read_text().split()
    assert len(checksum) == 2 and checksum[1].removeprefix("*") == archive.name, "checksum must use a bare archive filename"
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == checksum[0], "checksum mismatch"
    assert archive.name.endswith('.tar.gz'), 'release archives must use tar.gz'
    root = archive.name[:-7]
    with tarfile.open(archive) as bundle:
        files = [entry.name for entry in bundle.getmembers() if entry.isfile()]
        assert all(entry.isfile() or entry.isdir() for entry in bundle.getmembers()), "unexpected archive member type"
    required = {f"{root}/{name}" for name in ["oy", "LICENSE", "NOTICE",
                "README.md", "README.txt", "CHANGELOG.md", "SECURITY.md", "examples/olyx.json", "examples/request.json", "examples/providers.json", "examples/fallbacks.json", "examples/rag.json", "examples/rag.py", "docs/storage.md", "docs/providers.md", "docs/deployment.md", "docs/rag.md", "docs/diagnostics.md", "examples/ollama.json", "examples/plugins/echo.py", "docs/plugins.md", "docs/provider-testing.md", "scripts/test-plugin", "scripts/certify-providers.py"]}
    assert len(files) == len(set(files)), "duplicate archive member"
    assert set(files) == required, "archive contents do not match the public release allowlist"
    assert all(".." not in PurePosixPath(name).parts and not PurePosixPath(name).is_absolute() for name in files)
    print(f"Archive and checksum verified: {archive.name}")


if __name__ == "__main__":
    main()
