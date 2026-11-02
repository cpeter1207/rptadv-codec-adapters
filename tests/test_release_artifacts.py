"""Release staging keeps declared runtime packages and tracked source only."""

import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ReleaseArtifacts(unittest.TestCase):
    def test_release_stages_every_declared_package_and_tracked_source(self):
        control = (ROOT / "debian/control").read_text()
        packages = [
            line.removeprefix("Package: ")
            for line in control.splitlines()
            if line.startswith("Package: ")
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for directory in ("debian", "rust", "packaging/external", ".work"):
                (root / directory).mkdir(parents=True)
            shutil.copyfile(ROOT / "Makefile", root / "Makefile")
            (root / "debian/control").write_text(control)
            (root / "README.md").write_text("tracked source\n")
            (root / ".work/private").write_text("untracked evidence\n")
            expected = {f"{package}_test_amd64.deb" for package in packages}
            for command in (
                ["git", "init", "--quiet"],
                ["git", "add", "Makefile", "README.md", "debian/control"],
                [
                    "git",
                    "-c",
                    "user.name=Release Test",
                    "-c",
                    "user.email=release@example.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "fixture",
                ],
            ):
                subprocess.run(command, cwd=root, check=True, capture_output=True)
            clean_dist = subprocess.run(
                ["make", "--no-print-directory", "dist"],
                cwd=root,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(clean_dist.returncode, 0, clean_dist.stderr)
            for name in expected | {"librptadv-codec-g7111-dbgsym_test_amd64.deb"}:
                (root / "build" / name).write_text(name)
            result = subprocess.run(
                [
                    "make",
                    "--no-print-directory",
                    "-o",
                    "package-check",
                    "-o",
                    "package-artifact-check",
                    "release-packages",
                ],
                cwd=root,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            staged = root / "build/debian-source"
            self.assertEqual({path.name for path in staged.iterdir()}, expected)
            for name in expected:
                self.assertEqual((staged / name).read_text(), name)
            with tarfile.open(
                root / "build/rptadv-codec-adapters-0.1.0-alpha1.tar.gz"
            ) as archive:
                names = archive.getnames()
                readme = archive.extractfile(
                    "rptadv-codec-adapters-0.1.0-alpha1/README.md"
                )
                self.assertEqual(readme.read(), b"tracked source\n")
                self.assertFalse(
                    any(".work" in name or "/build/" in name for name in names)
                )


if __name__ == "__main__":
    unittest.main()
