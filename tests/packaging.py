#!/usr/bin/env python3
"""Check the package definitions without building anything.

Covers the pkgsmith scripts (pkgsmith/), the systemd units and preset, the desktop entries, the AppStream
files and the man pages. The packages themselves come from scripts/build-prefix.sh and
scripts/build-packages.sh (docs/packaging.md). The tests that run pkgsmith need it on PATH, or in
PKGSMITH; scripts/install-pkgsmith.sh builds it. They are skipped without it.
"""

import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# The release being packaged: every package, metainfo file and man page must say so.
VERSION = re.search(r"^VERSION \?= (\S+)", (ROOT / "Makefile").read_text(), re.M).group(1)
PKGSMITH = os.environ.get("PKGSMITH") or shutil.which("pkgsmith")
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
# The packages: `bingux` is the desktop with every core service; each application is its own package.
PACKAGES = ("bingux", "bingux-settings", "bingux-files", "bingux-calculator", "bingux-taxonomy")
# Application packages: desktop entry directory and metainfo id.
APPLICATIONS = {
    "bingux-settings": ("apps/settings", "dev.drewett.bingux.Settings"),
    "bingux-files": ("apps/files", "dev.drewett.bingux.Files"),
    "bingux-calculator": ("apps/calculator", "dev.drewett.bingux.Calculator"),
    "bingux-taxonomy": ("apps/taxonomy", "dev.drewett.bingux.Taxonomy"),
}
# The packages `bingux` replaces: the split of 0.1.0.
OLD_NAMES = ("bingux-config", "bingux-audio-effects", "bingux-preview-host")
MAN_PAGES = (
    "bingux-settings",
    "bingux-files",
    "bingux-calculator",
    "bingux-taxonomy",
    "bingux-config",
    "bingux-effects",
)


class UnitFilesTest(unittest.TestCase):
    """Every packaged service must run without core dumps and restart after a crash."""

    def test_services_disable_cores_and_restart_on_failure(self):
        services = sorted((ROOT / "data/systemd").glob("*.service"))
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
            for line in (ROOT / "data/systemd/90-bingux.preset").read_text().splitlines()
            if line.strip() and not line.startswith("#")
        ]
        self.assertIn("enable bingux.target", lines)
        self.assertIn("disable bingux-effects@.service", lines)
        self.assertEqual([line for line in lines if line.startswith("enable ")], ["enable bingux.target"])

    def test_bsm_units_enable_nothing(self):
        # Bingux OS only, but an administrator who installs bsm must not lose their login manager (docs/bsm/packaging.md).
        for path in sorted((ROOT / "data/bsm/systemd").glob("bsm*.service")):
            code = [line for line in path.read_text().splitlines() if not line.startswith("#")]
            self.assertNotIn("Alias=display-manager.service", code, path.name)

    def test_the_helper_target_wants_no_login_screen_unit(self):
        # The wallpaper units of the login screen belong to the Bingux OS image; its drop-in adds them.
        target = (ROOT / "data/systemd/bingux.target").read_text()
        wants = next(line for line in target.splitlines() if line.startswith("Wants="))
        self.assertNotIn("greeter", wants)
        self.assertNotIn("bsm", wants)
        self.assertFalse(list((ROOT / "data/systemd").glob("*greeter*")))
        self.assertFalse(list((ROOT / "data/systemd").glob("*bsm*")))
        dropin = (ROOT / "data/os/systemd/bingux.target.d/10-bingux-os.conf").read_text()
        self.assertIn("bingux-greeter-export.service", dropin)


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
            path = ROOT / "data/metainfo" / f"{component}.metainfo.xml"
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
                    str(ROOT / "data/metainfo" / f"{component}.metainfo.xml"),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class ManPageTest(unittest.TestCase):
    def test_every_command_has_a_page_with_the_standard_sections(self):
        for name in MAN_PAGES:
            text = (ROOT / "data/man" / f"{name}.1").read_text()
            self.assertTrue(text.startswith(f".TH {name.upper()} 1 "), name)
            self.assertIn(f'"Bingux {VERSION}"', text.splitlines()[0], name)
            for section in ("NAME", "SYNOPSIS", "DESCRIPTION"):
                self.assertIn(f".SH {section}", text, name)


class ScriptsTest(unittest.TestCase):
    """The pkgsmith scripts, read as text."""

    @classmethod
    def setUpClass(cls):
        cls.common = (ROOT / "pkgsmith/common.star").read_text()
        cls.bingux = (ROOT / "pkgsmith/bingux.star").read_text()
        cls.apps = (ROOT / "pkgsmith/apps.star").read_text()

    def test_the_default_version_is_the_makefile_version(self):
        self.assertIn(f'VERSION = arg("VERSION", "{VERSION}")', self.common)

    def test_the_packages_are_the_four_of_the_split(self):
        table = re.search(r"^LICENSES = \{(.*?)^\}", self.common, re.M | re.S).group(1)
        self.assertEqual(sorted(re.findall(r'^\s+"([^"]+)":', table, re.M)), sorted(PACKAGES))
        self.assertEqual(sorted(APPLICATIONS), sorted(re.findall(r'^    "(bingux-[a-z]+)": \{"id"', self.common, re.M)))

    def test_the_core_package_replaces_what_it_absorbed(self):
        for name in OLD_NAMES:
            self.assertIn(f'"{name}"', self.bingux)

    def test_the_core_package_holds_nothing_of_bingux_os(self):
        # bsm, the login screen, greetd and the boot splash are in the image of distro/, not in the package.
        text = re.sub(r"(?m)^#.*$", "", self.bingux)
        for word in ("bsm", "greetd", "greeter", "plymouth", "seatd", "libseat"):
            self.assertNotIn(word, text.lower(), word)

    def test_every_format_of_a_distribution_is_written_and_no_apk(self):
        self.assertIn('FORMATS = ["rpm", "deb", "archlinux", "tar"]', self.common)
        for text in (self.common, self.bingux, self.apps):
            self.assertNotIn('"apk"', text.replace("There is no apk", ""))

    def test_gnoblin_is_a_dependency_of_every_format(self):
        for fmt in ("rpm", "deb", "archlinux"):
            block = re.search(rf'^    "{fmt}": \[(.*?)^    \],', self.bingux, re.M | re.S).group(1)
            for dependency in ("gnoblin", "xdg-desktop-portal-gnoblin"):
                self.assertIn(f'"{dependency}"', block, fmt)

    def test_no_obsolete_packaging_is_left(self):
        self.assertFalse((ROOT / "packaging").exists())
        for path in ("scripts/build-rpms.sh", "scripts/build-arch.sh", "scripts/build-debs.sh"):
            self.assertFalse((ROOT / path).exists(), path)

    @unittest.skipUnless(shutil.which("cargo"), "cargo is not installed")
    def test_license_expressions_match_the_linked_crates(self):
        result = subprocess.run(
            ["python3", "scripts/cargo-licenses.py", "--out", tempfile.mkdtemp(), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


# The files of a prefix that the split depends on, with what ships each: one empty file of every kind.
def fake_prefix(root):
    paths = [
        "usr/bin/bingux-shell",
        "usr/bin/bingux-config",
        "usr/bin/bingux-effects",
        "usr/bin/bingux-preview-host",
        "usr/lib/systemd/user/bingux.target",
        "usr/lib/systemd/user/bingux-effects@.service",
        "usr/share/bingux/search.json",
        "usr/share/man/man1/bingux-config.1.gz",
        "usr/share/doc/bingux/README.md",
        "usr/share/licenses/bingux/COPYING",
        "etc/pam.d/bingux-lock",
    ]
    for name, (_, component) in APPLICATIONS.items():
        paths += [
            f"usr/bin/{name}",
            f"usr/share/applications/{name}.desktop",
            f"usr/share/icons/hicolor/scalable/apps/{name}.svg",
            f"usr/share/metainfo/{component}.metainfo.xml",
            f"usr/share/man/man1/{name}.1.gz",
            f"usr/share/doc/{name}/{name}.md",
            f"usr/share/licenses/{name}/COPYING",
        ]
    paths.append("usr/share/bingux/search-apps/bingux-settings.json")
    for path in paths:
        target = root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("x")


@unittest.skipUnless(PKGSMITH, "pkgsmith is not installed (scripts/install-pkgsmith.sh builds it)")
class PkgsmithTest(unittest.TestCase):
    """Run the scripts on a prefix that holds one file of every kind, and read what pkgsmith plans."""

    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.prefix = Path(cls.temp.name) / "prefix"
        fake_prefix(cls.prefix)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def pkgsmith(self, command, script, *arguments):
        result = subprocess.run(
            [
                PKGSMITH,
                command,
                "-f",
                str(ROOT / "pkgsmith" / f"{script}.star"),
                "--prefix",
                str(self.prefix),
                *arguments,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def plan(self, script, *arguments):
        return json.loads(self.pkgsmith("plan", script, "--json", *arguments))

    def test_each_script_writes_its_packages_in_every_format(self):
        core = self.plan("bingux")
        self.assertEqual(sorted({row["format"] for row in core}), ["archlinux", "deb", "rpm", "tar"])
        self.assertEqual({row["values"]["name"] for row in core}, {"bingux"})
        apps = self.plan("apps")
        self.assertEqual({row["values"]["name"] for row in apps}, set(APPLICATIONS))
        self.assertEqual(len(apps), 4 * len(APPLICATIONS))

    def test_no_file_is_in_two_packages_and_every_file_is_in_one(self):
        owners = {}
        for script in ("bingux", "apps"):
            listing = self.pkgsmith("files", script, "-F", "rpm")
            package = None
            for line in listing.splitlines():
                if line.startswith("=="):
                    package = re.search(r"name=([\w.+-]+)", line).group(1)
                elif re.match(r"\d+ root:root /", line):
                    path = line.split()[2]
                    # Only files count here: folders are owned by one package each (own_dirs), checked by the file lists.
                    if not (self.prefix / path.lstrip("/")).is_file():
                        continue
                    self.assertNotIn(path, owners, f"{path} is in {owners.get(path)} and {package}")
                    owners[path] = package
        for path in self.prefix.rglob("*"):
            if path.is_file():
                self.assertIn("/" + str(path.relative_to(self.prefix)), owners, f"{path} is in no package")
        for name in APPLICATIONS:
            self.assertEqual(owners[f"/usr/bin/{name}"], name)
        self.assertEqual(owners["/usr/bin/bingux-config"], "bingux")
        self.assertEqual(owners["/usr/bin/bingux-effects"], "bingux")
        self.assertEqual(owners["/usr/bin/bingux-preview-host"], "bingux")
        self.assertEqual(owners["/usr/lib/systemd/user/bingux.target"], "bingux")
        self.assertEqual(owners["/etc/pam.d/bingux-lock"], "bingux")
        self.assertEqual(owners["/usr/share/bingux/search-apps/bingux-settings.json"], "bingux-settings")

    def test_the_core_package_lists_no_file_of_bingux_os(self):
        listing = self.pkgsmith("files", "bingux", "-F", "rpm,deb,archlinux")
        for word in ("bsm", "greetd", "greeter", "plymouth", "seatd"):
            self.assertNotIn(word, listing.lower(), word)

    def build(self, script, version, formats):
        output = Path(self.temp.name) / f"out-{script}-{version}"
        subprocess.run(
            [
                PKGSMITH,
                "build",
                "-q",
                "-f",
                str(ROOT / "pkgsmith" / f"{script}.star"),
                "--prefix",
                str(self.prefix),
                "-F",
                formats,
                "-o",
                str(output),
                "--build-arg",
                f"VERSION={version}",
                "--build-arg",
                "RPM_DIST=.fc43",
                "--build-arg",
                "QT_MINOR=6.10",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        return output

    def test_the_apps_pin_the_exact_version_of_bingux_in_the_spelling_of_each_format(self):
        # The script spells the version of each format itself (native_version in common.star). Compare that with the
        # version pkgsmith writes into the file name and the metadata of bingux, for a release and a nightly.
        for version in ("0.1.0", "0.1.1-nightly.20261031.142.gabc1234"):
            core = self.build("bingux", version, "rpm,deb,archlinux")
            apps = self.build("apps", version, "rpm,deb,archlinux")
            if shutil.which("rpm"):
                rpm = next(core.glob("bingux-[0-9]*.rpm")).name
                native = rpm[len("bingux-") :].rsplit(".", 2)[0]
                requires = subprocess.run(
                    ["rpm", "-qp", "--requires", str(next(apps.glob("bingux-files-*.rpm")))],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout.splitlines()
                self.assertIn(f"bingux = {native}", requires)
            if shutil.which("dpkg-deb"):
                deb = next(core.glob("bingux_*.deb")).name
                native = deb[len("bingux_") :].rsplit("_", 1)[0]
                depends = subprocess.run(
                    ["dpkg-deb", "-f", str(next(apps.glob("bingux-files_*.deb"))), "Depends"],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout
                self.assertIn(f"bingux (= {native})", depends)
            if shutil.which("bsdtar"):
                arch = next(core.glob("bingux-[0-9]*.pkg.tar.zst")).name
                native = arch[len("bingux-") :].rsplit("-", 1)[0]
                info = subprocess.run(
                    ["bsdtar", "-xOf", str(next(apps.glob("bingux-files-*.pkg.tar.zst"))), ".PKGINFO"],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout.splitlines()
                self.assertIn(f"depend = bingux={native}", info)

    def test_the_core_package_replaces_the_old_names_in_every_format(self):
        core = self.build("bingux", "0.1.0", "rpm,deb,archlinux")
        if shutil.which("rpm"):
            obsoletes = subprocess.run(
                ["rpm", "-qp", "--obsoletes", str(next(core.glob("*.rpm")))], capture_output=True, text=True, check=True
            ).stdout.split()
            self.assertEqual(sorted(obsoletes), sorted(OLD_NAMES))
        if shutil.which("dpkg-deb"):
            for field in ("Replaces", "Conflicts", "Provides"):
                value = subprocess.run(
                    ["dpkg-deb", "-f", str(next(core.glob("*.deb"))), field], capture_output=True, text=True, check=True
                ).stdout
                self.assertEqual(sorted(name.strip() for name in value.split(",")), sorted(OLD_NAMES), field)
        if shutil.which("bsdtar"):
            info = subprocess.run(
                ["bsdtar", "-xOf", str(next(core.glob("*.pkg.tar.zst"))), ".PKGINFO"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.splitlines()
            for key in ("replaces", "conflict", "provides"):
                self.assertEqual(
                    sorted(line.split(" = ")[1] for line in info if line.startswith(key + " = ")),
                    sorted(OLD_NAMES),
                    key,
                )


if __name__ == "__main__":
    unittest.main()
