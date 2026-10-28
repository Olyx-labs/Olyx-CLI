"""Reject private/local runtime dependencies in the public CLI's resolved graph."""
import json
from pathlib import Path
import subprocess

PUBLIC_PACKAGES = {"olyx-cli", "olyx-local-runtime"}
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def validate(metadata, root):
    root = root.resolve()
    members = set(metadata["workspace_members"])
    packages = metadata["packages"]
    workspace_names = {p["name"] for p in packages if p["id"] in members}
    if workspace_names != PUBLIC_PACKAGES:
        raise ValueError(f"Unexpected public workspace packages: {workspace_names}")
    for package in packages:
        name = package["name"].replace("_", "-")
        source = package.get("source")
        if name.startswith("olyx-") and name not in PUBLIC_PACKAGES:
            raise ValueError(f"Private Olyx package in dependency graph: {name}")
        if source is None:
            if name not in PUBLIC_PACKAGES or package["id"] not in members:
                raise ValueError(f"Unapproved local dependency: {name}")
            if not Path(package["manifest_path"]).resolve().is_relative_to(root):
                raise ValueError(f"Workspace package outside public repository: {name}")
        elif source != CRATES_IO:
            raise ValueError(f"Unapproved dependency source for {name}: {source}")


def main():
    root = Path(__file__).resolve().parents[1]
    result = subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root, check=True, text=True, capture_output=True)
    validate(json.loads(result.stdout), root)
    print("Public dependency boundary verified: CLI + local runtime; no private runtime packages.")


if __name__ == "__main__":
    main()
