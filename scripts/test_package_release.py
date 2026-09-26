"""Focused checks for release license collection without a Cargo build."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("package_release", Path(__file__).with_name("package-release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class DependencyNoticesTest(unittest.TestCase):
    def test_texts_exceptions_and_repeatable_inventory(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "crate"
            source.mkdir()
            (source / "Cargo.toml").write_text("[package]\n")
            (source / "LICENSE-MIT").write_bytes(b"Full license\n")
            (source / "NOTICE").write_bytes(b"Attribution\n")
            (source / "other").write_text("not a notice")
            p = {"name": "example", "version": "1.0.0", "license": "MIT", "license_file": None,
                 "manifest_path": str(source / "Cargo.toml"), "source": "registry"}
            missing = dict(p, name="metadata-only", license="Apache-2.0")
            other = base / "other"
            other.mkdir()
            (other / "Cargo.toml").touch()
            missing["manifest_path"] = str(other / "Cargo.toml")
            with patch.object(release, "dependency_packages", return_value=[p, p, missing]):
                a, b = base / "a", base / "b"
                release.dependency_notices(a, "target")
                release.dependency_notices(b, "target")
            self.assertEqual((a / "rust-dependencies.txt").read_bytes(), (b / "rust-dependencies.txt").read_bytes())
            self.assertEqual((a / "dependencies/example-1.0.0/LICENSE-MIT").read_bytes(), b"Full license\n")
            self.assertEqual((a / "dependencies/example-1.0.0/NOTICE").read_bytes(), b"Attribution\n")
            self.assertIn("metadata-only 1.0.0 | Apache-2.0 | registry | METADATA-ONLY", (a / "rust-dependencies.txt").read_text())

    def test_undeclared_and_unsafe_paths_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "crate"
            source.mkdir()
            (source / "Cargo.toml").touch()
            p = {"name": "example", "version": "1.0", "license": None, "license_file": None,
                 "manifest_path": str(source / "Cargo.toml"), "source": None}
            with patch.object(release, "dependency_packages", return_value=[p]):
                with self.assertRaisesRegex(RuntimeError, "undeclared"):
                    release.dependency_notices(base / "out", "target")
            outside = base / "outside"
            outside.write_text("foreign license")
            p["license"] = "MIT"
            p["license_file"] = "../outside"
            with self.assertRaisesRegex(RuntimeError, "invalid license file"):
                release.license_sources(p)
            p["license_file"] = None
            (source / "LICENSE").symlink_to(outside)
            with self.assertRaisesRegex(RuntimeError, "invalid license file"):
                release.license_sources(p)
            p["name"] = "../escape"
            with patch.object(release, "dependency_packages", return_value=[p]):
                with self.assertRaisesRegex(RuntimeError, "unsafe crate"):
                    release.dependency_notices(base / "out", "target")

    def test_named_license_file_and_conflicting_package_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            packages = []
            for index, text in enumerate(("first", "second")):
                root = base / str(index)
                (root / "legal").mkdir(parents=True)
                (root / "Cargo.toml").touch()
                (root / "legal" / "terms.txt").write_text(text)
                packages.append({"name": "same", "version": "1.0", "license": None,
                                 "license_file": "legal/terms.txt", "manifest_path": str(root / "Cargo.toml"),
                                 "source": f"source-{index}"})
            with patch.object(release, "dependency_packages", return_value=packages[:1]):
                release.dependency_notices(base / "good", "target")
            self.assertEqual((base / "good/dependencies/same-1.0/legal/terms.txt").read_text(), "first")
            with patch.object(release, "dependency_packages", return_value=packages):
                with self.assertRaisesRegex(RuntimeError, "conflicting"):
                    release.dependency_notices(base / "conflict", "target")


if __name__ == "__main__":
    unittest.main()
