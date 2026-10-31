#!/usr/bin/env python3
"""Check the package definitions without building anything.

Covers the RPM spec, the Arch PKGBUILD, the Debian source package, the systemd
units and preset, the desktop entries, the AppStream files and the man pages.
`make rpm-package` and `make arch-package` build the packages themselves, and
`scripts/build-debs.sh` builds the Debian ones.
"""

import re
import shutil
import subprocess
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# The release being packaged: every package, metainfo file and man page must say so.
VERSION = re.search(r"^VERSION \?= (\S+)", (ROOT / "Makefile").read_text(), re.M).group(1)
UNITS = (
    "bingux.target",
    "bingux.service",
    "bingux-searchd.service",
    "bingux-statusd.service",
    "bingux-search-ui.service",
    "bingux-switcher-ui.service",
    "bingux-capture-ui.service",
    "bingux-emoji-ui.service",
    "xdg-desktop-portal-bingux.service",
)
PACKAGES = (
    "bingux",
    "bingux-config",
    "bingux-settings",
    "bingux-files",
    "bingux-preview-host",
    "bingux-audio-effects",
    "bingux-taxonomy",
)
# Application packages: binary, desktop entry, metainfo id and man page.
APPLICATIONS = {
    "bingux-settings": ("apps/settings", "dev.drewett.bingux.Settings"),
    "bingux-files": ("apps/files", "dev.drewett.bingux.Files"),
    "bingux-taxonomy": ("apps/taxonomy", "dev.drewett.bingux.Taxonomy"),
}
MAN_PAGES = ("bingux-settings", "bingux-files", "bingux-taxonomy", "bingux-config", "bingux-effects")


class UnitFilesTest(unittest.TestCase):
    """Every packaged service must run without core dumps and restart after a crash."""

    def test_services_disable_cores_and_restart_on_failure(self):
        services = sorted((ROOT / "packaging/systemd").glob("*.service"))
        self.assertTrue(services)
        for path in services:
            lines = path.read_text().splitlines()
            self.assertIn("LimitCORE=0", lines, path.name)
            if path.name == "bingux-lock.service":
                # The lock client is a manual diagnostic unit and must not respawn.
                self.assertIn("Restart=no", lines)
            else:
                self.assertIn("Restart=on-failure", lines, path.name)

    def test_preset_enables_the_helper_target_only(self):
        lines = [
            line
            for line in (ROOT / "packaging/systemd/90-bingux.preset").read_text().splitlines()
            if line.strip() and not line.startswith("#")
        ]
        self.assertIn("enable bingux.target", lines)
        self.assertIn("disable bingux-effects@.service", lines)
        self.assertEqual([line for line in lines if line.startswith("enable ")], ["enable bingux.target"])


class DesktopFilesTest(unittest.TestCase):
    @unittest.skipUnless(shutil.which("desktop-file-validate"), "desktop-file-validate is not installed")
    def test_desktop_entries_validate(self):
        for package, (directory, _) in APPLICATIONS.items():
            result = subprocess.run(
                ["desktop-file-validate", str(ROOT / directory / f"{package}.desktop")],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_entries_match_window_id_icon_and_scheme(self):
        for package, (directory, _) in APPLICATIONS.items():
            entry = (ROOT / directory / f"{package}.desktop").read_text()
            self.assertIn(f"Icon={package}\n", entry)
            # The window's app id is the program name, so the dock groups by it.
            self.assertIn(f"StartupWMClass={package}\n", entry)
            self.assertIn(f"x-scheme-handler/{package}", entry)
            self.assertTrue((ROOT / directory / f"{package}.svg").is_file())
        files = (ROOT / "apps/files/bingux-files.desktop").read_text()
        self.assertIn("inode/directory", files)


class AppStreamTest(unittest.TestCase):
    def test_every_application_has_matching_metainfo(self):
        for package, (_, component) in APPLICATIONS.items():
            path = ROOT / "packaging/metainfo" / f"{component}.metainfo.xml"
            tree = ET.parse(path).getroot()
            self.assertEqual(tree.findtext("id"), component)
            self.assertEqual(tree.findtext("launchable"), f"{package}.desktop")
            self.assertEqual(tree.findtext("provides/binary"), package)
            self.assertEqual(tree.findtext("project_license"), "GPL-3.0-or-later")
            self.assertEqual(tree.find("releases/release").get("version"), VERSION)

    @unittest.skipUnless(shutil.which("appstreamcli"), "appstreamcli is not installed")
    def test_metainfo_validates(self):
        for _, component in APPLICATIONS.values():
            result = subprocess.run(
                [
                    "appstreamcli",
                    "validate",
                    "--no-net",
                    str(ROOT / "packaging/metainfo" / f"{component}.metainfo.xml"),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class ManPageTest(unittest.TestCase):
    def test_every_command_has_a_page_with_the_standard_sections(self):
        for name in MAN_PAGES:
            text = (ROOT / "packaging/man" / f"{name}.1").read_text()
            self.assertTrue(text.startswith(f".TH {name.upper()} 1 "), name)
            self.assertIn(f'"Bingux {VERSION}"', text.splitlines()[0], name)
            for section in ("NAME", "SYNOPSIS", "DESCRIPTION"):
                self.assertIn(f".SH {section}", text, name)


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
        names = subprocess.run(
            ["rpmspec", "-q", "--qf", "%{name}\\n", "packaging/rpm/bingux.spec"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        cls.names = names.stdout.split()

    def test_the_spec_builds_every_package(self):
        self.assertEqual(sorted(self.names), sorted(PACKAGES))

    def test_user_unit_lifecycle_is_expanded(self):
        for action in ("install-user-units", "remove-user-units", "mark-restart-user-units"):
            self.assertIn(action, self.spec)
            for unit in UNITS:
                self.assertIn(unit, self.spec)

    def test_effects_instances_are_stopped_on_removal(self):
        for unit in ("bingux-effects@output.service", "bingux-effects@input.service"):
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
        self.assertTrue((ROOT / "gnoblin/bingux.lua").is_file())
        self.assertIn("/usr/share/gnoblin/conf.d/bingux.lua", self.spec)

    def test_portal_backend_is_packaged_and_requires_the_portal(self):
        # The backend unit is not covered by the bingux*.service glob, so name it.
        self.assertIn("Requires:       xdg-desktop-portal\n", self.spec)
        files = self.spec.split("\n%files\n", 1)[1].split("\n%changelog", 1)[0].splitlines()
        self.assertIn("/usr/lib/systemd/user/xdg-desktop-portal-bingux.service", files)

    def test_each_application_package_ships_its_entry_icon_metainfo_and_man_page(self):
        for package, (_, component) in APPLICATIONS.items():
            for path in (
                f"/usr/bin/{package}",
                f"/usr/share/applications/{package}.desktop",
                f"/usr/share/icons/hicolor/scalable/apps/{package}.svg",
                f"/usr/share/metainfo/{component}.metainfo.xml",
                f"/usr/share/man/man1/{package}.1*",
            ):
                self.assertIn(path, self.spec)

    def test_the_preview_host_is_its_own_package_with_the_decoders_it_links(self):
        package = re.search(r"^%package preview-host\n(.*?)^%description", self.spec, re.M | re.S).group(1)
        for requirement in ("gstreamer1-plugins-base", "gstreamer1-plugins-good", "poppler-glib"):
            self.assertIn(f"Requires:       {requirement}\n", package)
        for recommendation in ("gstreamer1-plugin-libav", "gstreamer1-plugins-bad-free"):
            self.assertIn(f"Recommends:     {recommendation}\n", package)
        self.assertIn("/usr/bin/bingux-preview-host", self.spec)
        # Files works without it (it falls back to the card), so it is only recommended.
        files = re.search(r"^%package files\n(.*?)^%description", self.spec, re.M | re.S).group(1)
        self.assertIn("Recommends:     bingux-preview-host", files)
        self.assertNotIn("Requires:       bingux-preview-host", files)
        for build_requirement in ("gstreamer-1.0", "gstreamer-app-1.0", "gstreamer-video-1.0", "poppler-glib"):
            self.assertIn(f"BuildRequires:  pkgconfig({build_requirement})", self.spec)

    def test_no_adwaita_icons_are_installed(self):
        self.assertNotIn("Adwaita", self.spec.replace("adwaita-sans-fonts", ""))
        self.assertNotIn("adwaita-icon-theme", self.spec)
        self.assertNotIn("/share/bingux/icons", self.spec)
        self.assertIn("/licenses/bingux/icons/COPYING", self.spec)

    def test_no_scriptlet_hand_updates_desktop_caches(self):
        code = "\n".join(line for line in self.spec.splitlines() if not line.lstrip().startswith("#"))
        for command in (
            "update-desktop-database",
            "gtk-update-icon-cache",
            "update-mime-database",
            "glib-compile-schemas",
        ):
            self.assertNotIn(command, code)

    @unittest.skipUnless(shutil.which("cargo"), "cargo is not installed")
    def test_license_tags_match_the_linked_crates(self):
        result = subprocess.run(
            ["python3", "scripts/cargo-licenses.py", "--out", "build/licenses", "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


class ArchPackageTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = (ROOT / "packaging/arch/PKGBUILD").read_text()

    def test_syntax(self):
        subprocess.run(["bash", "-n", str(ROOT / "packaging/arch/PKGBUILD")], check=True)
        subprocess.run(["bash", "-n", str(ROOT / "packaging/arch/bingux.install")], check=True)

    def test_builds_the_same_packages_as_the_spec(self):
        match = re.search(r"^pkgname=\(([^)]*)\)", self.text, re.M)
        self.assertIsNotNone(match)
        self.assertEqual(sorted(match.group(1).split()), sorted(PACKAGES))
        for name in PACKAGES:
            self.assertIn(f"package_{name}()", self.text)

    def test_the_shell_is_last_so_it_takes_what_is_left(self):
        match = re.search(r"^pkgname=\(([^)]*)\)", self.text, re.M)
        self.assertEqual(match.group(1).split()[-1], "bingux")

    def test_version_matches_the_makefile_and_the_spec(self):
        makefile = (ROOT / "Makefile").read_text()
        spec = (ROOT / "packaging/rpm/bingux.spec").read_text()
        version = re.search(r"^VERSION \?= (\S+)", makefile, re.M).group(1)
        self.assertIn(f"pkgver={version}\n", self.text)
        self.assertIn(f"Version:        {version}\n", spec)


class DebianPackageTest(unittest.TestCase):
    """packaging/debian splits the install into the same packages as the spec and the PKGBUILD."""

    def setUp(self):
        self.control = (ROOT / "packaging/debian/control").read_text()

    def test_builds_the_same_packages_as_the_spec(self):
        names = re.findall(r"^Package: (\S+)$", self.control, re.M)
        self.assertEqual(sorted(names), sorted(PACKAGES))

    def test_every_package_has_an_install_list_and_none_claims_a_file_twice(self):
        claimed = {}
        for name in PACKAGES:
            install = ROOT / f"packaging/debian/{name}.install"
            self.assertTrue(install.exists(), install.name)
            for line in install.read_text().splitlines():
                path = line.split()[0] if line.strip() and not line.startswith("#") else None
                if path and path.startswith("usr/") and not path.endswith("*"):
                    self.assertNotIn(path, claimed, f"{path} is in {claimed.get(path)} and {name}")
                    claimed[path] = name

    def test_the_small_packages_list_what_the_pkgbuild_takes(self):
        pkgbuild = (ROOT / "packaging/arch/PKGBUILD").read_text()
        for name in PACKAGES[1:]:
            body = re.search(rf"^package_{name}\(\) \{{(.*?)^\}}", pkgbuild, re.M | re.S).group(1)
            taken = set(re.findall(r"\b(usr/[\w./@-]+)", body))
            listed = {
                line.split()[0]
                for line in (ROOT / f"packaging/debian/{name}.install").read_text().splitlines()
                if line.startswith("usr/")
            }
            self.assertEqual(listed, taken, name)

    def test_the_shell_installs_last_and_keeps_what_is_left(self):
        rules = (ROOT / "packaging/debian/rules").read_text()
        self.assertLess(rules.index("dh_install $(addprefix -p,$(SMALL))"), rules.index("dh_install -pbingux"))
        self.assertIn("dh_missing --fail-missing", rules)

    def test_the_changelog_names_the_version_in_the_makefile(self):
        changelog = (ROOT / "packaging/debian/changelog").read_text()
        self.assertTrue(changelog.startswith(f"bingux ({VERSION}-1) "), changelog.splitlines()[0])


if __name__ == "__main__":
    unittest.main()
