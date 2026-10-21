#!/usr/bin/env python3
"""Verify the RPM spec keeps the managed user-service lifecycle intact."""

from pathlib import Path
import shutil
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
UNITS = (
    "bingux.target",
    "bingux.service",
    "bingux-searchd.service",
    "bingux-statusd.service",
    "bingux-search-ui.service",
    "bingux-switcher-ui.service",
    "bingux-capture-ui.service",
    "bingux-emoji-ui.service",
)


@unittest.skipUnless(shutil.which("rpmspec"), "rpmspec is not installed")
class RpmPackageTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run(
            ["rpmspec", "-P", "packaging/rpm/bingux.spec"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise AssertionError(result.stderr or result.stdout)
        cls.spec = result.stdout

    def test_user_unit_lifecycle_is_expanded(self):
        for action in ("install-user-units", "remove-user-units", "mark-restart-user-units"):
            self.assertIn(action, self.spec)
            for unit in UNITS:
                self.assertIn(unit, self.spec)

    def test_shell_wrappers_and_license_are_packaged(self):
        for wrapper in (
            "bingux-shell",
            "bingux",
            "bingux-capture-ui",
            "bingux-emoji-ui",
            "bingux-search-ui",
            "bingux-switcher-ui",
            "binguxctl",
            "bingux-frame",
        ):
            self.assertIn(f"/usr/bin/{wrapper}", self.spec)
        self.assertIn("%license COPYING", self.spec)

    def test_quickshell_is_built_in_not_required(self):
        # bingux-shell carries Quickshell; only its Qt runtime is a dependency.
        self.assertNotIn("/usr/bin/qs", self.spec)
        self.assertNotIn("Requires:       quickshell\n", self.spec)
        self.assertIn("Requires:       qt6-qtdeclarative", self.spec)
        for build_requirement in ("cmake", "ninja-build", "cli11-devel", "pkgconfig(libarchive)"):
            self.assertIn(f"BuildRequires:  {build_requirement}", self.spec)

    def test_gnoblin_fragment_is_in_the_payload(self):
        self.assertTrue((ROOT / "packaging/gnoblin/bingux.lua").is_file())
        self.assertIn("/usr/share/gnoblin/conf.d/bingux.lua", self.spec)


if __name__ == "__main__":
    unittest.main()
