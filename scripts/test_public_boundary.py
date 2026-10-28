"""Guard the public/private distribution boundary against dependency regressions."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("boundary", Path(__file__).with_name("verify-public-boundary.py"))
boundary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boundary)


class PublicBoundary(unittest.TestCase):
    root = Path(__file__).resolve().parents[1]

    def metadata(self):
        return {"workspace_members": ["cli", "runtime"], "packages": [
            {"id": "cli", "name": "olyx-cli", "source": None, "manifest_path": str(self.root / "Cargo.toml")},
            {"id": "runtime", "name": "olyx-local-runtime", "source": None, "manifest_path": str(self.root / "runtime/Cargo.toml")},
            {"id": "serde", "name": "serde", "source": boundary.CRATES_IO},
        ]}

    def test_public_workspace_passes(self):
        boundary.validate(self.metadata(), self.root)

    def test_private_transitive_package_is_rejected(self):
        data = self.metadata()
        data["packages"].append({"id": "private", "name": "olyx_core", "source": boundary.CRATES_IO})
        with self.assertRaises(ValueError):
            boundary.validate(data, self.root)

    def test_external_path_dependency_is_rejected(self):
        data = self.metadata()
        data["packages"].append({"id": "other", "name": "internal-runtime", "source": None, "manifest_path": str(self.root.parent / "private/Cargo.toml")})
        with self.assertRaises(ValueError):
            boundary.validate(data, self.root)

    def test_git_dependency_is_rejected(self):
        data = self.metadata()
        data["packages"][2]["source"] = "git+https://example.com/private"
        with self.assertRaises(ValueError):
            boundary.validate(data, self.root)

    def test_additional_workspace_member_is_rejected(self):
        data = self.metadata()
        data["workspace_members"].append("enterprise")
        data["packages"].append({"id": "enterprise", "name": "enterprise", "source": None})
        with self.assertRaises(ValueError):
            boundary.validate(data, self.root)


if __name__ == "__main__":
    unittest.main()
