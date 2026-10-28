"""Package an already-built CLI. Python and Rust are release-builder tools only."""
import argparse
import hashlib
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl", "aarch64-apple-darwin"}
DOCS = ["storage.md", "providers.md", "deployment.md", "rag.md", "diagnostics.md", "plugins.md", "provider-testing.md"]

def package(binary, version, target, output):
    if target not in TARGETS or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:-rehearsal)?", version):
        raise ValueError("unsupported target or invalid version")
    if not binary.is_file():
        raise ValueError(f"binary does not exist: {binary}")
    output.mkdir(parents=True, exist_ok=True)
    name = f"olyx-{version}-{target}"
    archive = output / (name + ".tar.gz")
    with tempfile.TemporaryDirectory(prefix="olyx-package-") as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        executable = stage / "oy"
        shutil.copyfile(binary, executable)
        executable.chmod(0o755)
        for item in ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "SECURITY.md"]:
            shutil.copyfile(ROOT / item, stage / item)
        shutil.copytree(ROOT / "examples", stage / "examples", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        for item in DOCS:
            destination = stage / "docs" / item
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / "docs" / item, destination)
        (stage / "scripts").mkdir()
        for item in ["test-plugin", "certify-providers.py"]:
            shutil.copy2(ROOT / "scripts" / item, stage / "scripts" / item)
        (stage / "README.txt").write_text(
            f"Olyx CLI {version} for {target}\n\n"
            "This is a prebuilt executable. Rust, Cargo, a C compiler, Node.js, Python,\n"
            "and a database server are not required to run it. SQLite is bundled.\n\n"
            "Verify the adjacent .sha256 file before extracting. A Sigstore .bundle\n"
            "is supplied with published releases for optional provenance verification.\n"
            "Put oy (Linux/macOS) in a directory on PATH.\n"
            "Run oy --version, then oy init --provider openai to get started.\n"
            "External provider plugins may require their own runtime.\n"
        )
        with tarfile.open(archive, "w:gz") as bundle:
            bundle.add(stage, arcname=name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    Path(str(archive) + ".sha256").write_bytes(f"{digest}  {archive.name}\n".encode("ascii"))
    return archive

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", choices=sorted(TARGETS), required=True)
    parser.add_argument("--output", type=Path, default=Path("dist"))
    args = parser.parse_args()
    print(package(args.binary, args.version, args.target, args.output))
