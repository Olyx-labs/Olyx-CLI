"""Run the executable and examples from an already-validated release archive."""
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

archive = Path(sys.argv[1]).resolve()
assert archive.name.endswith('.tar.gz')
name = archive.name[:-7]
with tempfile.TemporaryDirectory(prefix="olyx-packaged-smoke-") as directory:
    with tarfile.open(archive) as bundle:
        bundle.extractall(directory)
    root = Path(directory) / name
    binary = root / "oy"
    commands = [["--version"], ["help"], ["init", "--provider", "openai"], ["check", "olyx.json"], ["check", "examples/olyx.json"], ["check", "examples/fallbacks.json"], ["check", "examples/providers.json"], ["check", "examples/ollama.json"], ["check", "examples/rag.json"], ["route", "--config", "examples/olyx.json", "--input", "examples/request.json", "--dry-run"]]
    for args in commands:
        subprocess.run([str(binary), *args], cwd=root, check=True, capture_output=True, timeout=10)
print("Packaged binary smoke passed: executable, init, bundled configurations, and request dry-run.")
