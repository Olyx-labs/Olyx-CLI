"""Verify a release archive and its portable adjacent SHA-256 checksum."""
import hashlib
from pathlib import Path, PurePosixPath
import sys
import tarfile
import zipfile


def main():
    archive = Path(sys.argv[1])
    checksum = Path(str(archive) + ".sha256").read_text().split()
    assert len(checksum) == 2 and checksum[1] == archive.name, "checksum must use a bare archive filename"
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == checksum[0], "checksum mismatch"
    windows = archive.name.endswith(".zip")
    root = archive.name[:-4] if windows else archive.name[:-7]
    if windows:
        with zipfile.ZipFile(archive) as bundle:
            files = [entry.filename for entry in bundle.infolist() if not entry.is_dir()]
    else:
        with tarfile.open(archive) as bundle:
            files = [entry.name for entry in bundle.getmembers() if entry.isfile()]
            assert all(entry.isfile() or entry.isdir() for entry in bundle.getmembers()), "unexpected archive member type"
    required = {f"{root}/{name}" for name in ["olyx.exe" if windows else "olyx", "LICENSE", "NOTICE",
                "README.md", "README.txt", "examples/olyx.json", "examples/request.json"]}
    assert set(files) == required, "archive contents do not match the public release allowlist"
    assert all(".." not in PurePosixPath(name).parts and not PurePosixPath(name).is_absolute() for name in files)
    print(f"Archive and checksum verified: {archive.name}")


if __name__ == "__main__":
    main()
