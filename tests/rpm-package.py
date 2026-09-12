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
            "bingux",
            "bingux-capture-ui",
            "bingux-emoji-ui",
            "bingux-search-ui",
            "bingux-settings",
            "bingux-switcher-ui",
            "binguxctl",
        ):
            self.assertIn(f"/usr/bin/{wrapper}", self.spec)
        self.assertIn("%license COPYING", self.spec)

    def test_runtime_requires_the_quickshell_executable(self):
        self.assertIn("Requires:       /usr/bin/qs", self.spec)
        self.assertNotIn("Requires:       quickshell\n", self.spec)

    def test_gnoblin_fragment_is_in_the_payload(self):
        self.assertTrue((ROOT / "packaging/gnoblin/bingux.toml").is_file())
        self.assertIn("/usr/share/bingux/", self.spec)


if __name__ == "__main__":
    unittest.main()
