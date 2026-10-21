#!/usr/bin/env python3
"""Check that Bingux OS names itself, not Arch, where a user can see it.

usage: tests/distro-branding.py [--iso ISO] [--root DIR]

Always checks the sources in distro/profile and distro/packages/bingux-os:
the boot menus must not show "Arch Linux", live boot entries must ask for the
splash, and the package must ship the release identity, the alpm hook that
applies it and the Plymouth theme.

--iso ISO   also extracts the boot menus and the live root from a built image
            (xorriso and unsquashfs) and checks the same things there.
--root DIR  also checks an installed system mounted at DIR: os-release, issue,
            the mkinitcpio plymouth hook and the boot loader entries archinstall
            wrote.

Exits non-zero and lists every failure. Nothing is modified.
"""

import argparse
from pathlib import Path
import re
import subprocess
import sys
import tempfile
REPO = Path(__file__).resolve().parent.parent
PROFILE = REPO / "distro" / "profile"
PACKAGE = REPO / "distro" / "packages" / "bingux-os"

ARCH_NAME = re.compile(r"Arch Linux")
BOOT_MENU_FILES = [
    "grub/grub.cfg",
    "grub/loopback.cfg",
    "syslinux/archiso_head.cfg",
    "syslinux/archiso_sys-linux.cfg",
    "efiboot/loader/entries/01-archiso-linux.conf",
    "efiboot/loader/entries/02-archiso-speech-linux.conf",
]
SPLASH_OPTIONS = ("quiet", "splash")


def check_text(name, text, failures):
    for line in text.splitlines():
        if ARCH_NAME.search(line) and not line.lstrip().startswith("#"):
            failures.append(f"{name}: Arch name: {line.strip()}")


def check_sources(failures):
    for rel in BOOT_MENU_FILES:
        path = PROFILE / rel
        if not path.exists():
            failures.append(f"missing boot menu {rel}")
            continue
        text = path.read_text(errors="replace")
        check_text(rel, text, failures)
        # The title-only syslinux header has no boot options of its own.
        if rel != "syslinux/archiso_head.cfg" and "splash" not in text:
            failures.append(f"{rel}: live boot entries lack the splash option")

    for name in ("hostname", "motd"):
        check_text(f"airootfs/etc/{name}",
                   (PROFILE / "airootfs" / "etc" / name).read_text(errors="replace"), failures)

    os_release = (PACKAGE / "bingux-os-release").read_text()
    for want in ('NAME="Bingux OS"', "ID=bingux", "ID_LIKE=arch"):
        if want not in os_release:
            failures.append(f"os-release lacks {want}")

    issue = (PACKAGE / "bingux-os-issue").read_text()
    check_text("bingux-os-issue", issue, failures)

    hook = (PACKAGE / "bingux-os-release.hook").read_text()
    for want in ("Target = filesystem", "Target = bingux-os", "PostTransaction"):
        if want not in hook:
            failures.append(f"alpm hook lacks {want}")

    for rel in ("bingux.plymouth", "bingux.script"):
        if not (PACKAGE / rel).exists():
            failures.append(f"missing {rel}")
    if (PACKAGE / "plymouthd.conf").exists():
        failures.append("bingux-os ships plymouthd.conf, which plymouth owns")
    if "plymouth-set-default-theme -R bingux" not in (PACKAGE / "bingux-os.install").read_text():
        failures.append("install hook does not select the bingux theme")

    if "'plymouth'" not in (PACKAGE / "PKGBUILD").read_text():
        failures.append("PKGBUILD does not depend on plymouth")


def check_iso(iso, failures):
    with tempfile.TemporaryDirectory(prefix="branding-iso.") as tmp:
        out = Path(tmp)
        for path in ("/loader", "/syslinux", "/grub"):
            subprocess.run(
                ["xorriso", "-osirrox", "on", "-indev", str(iso),
                 "-extract", path, str(out / path.lstrip("/"))],
                check=False, capture_output=True,
            )
        menus = [p for p in out.rglob("*") if p.is_file() and p.suffix in (".conf", ".cfg")]
        if not menus:
            failures.append(f"{iso.name}: no boot menus extracted")
        for menu in menus:
            check_text(f"{iso.name}:{menu.relative_to(out)}",
                       menu.read_text(errors="replace"), failures)

        squash = out / "airootfs.sfs"
        subprocess.run(
            ["xorriso", "-osirrox", "on", "-indev", str(iso), "-extract",
             "/arch/x86_64/airootfs.sfs", str(squash)],
            check=False, capture_output=True,
        )
        if not squash.exists():
            failures.append(f"{iso.name}: airootfs.sfs not found")
            return
        extract = out / "root"
        subprocess.run(
            ["unsquashfs", "-q", "-f", "-d", str(extract), str(squash),
             "etc/os-release", "etc/issue", "etc/motd", "etc/hostname"],
            check=False, capture_output=True,
        )
        for rel in ("etc/os-release", "etc/issue", "etc/motd", "etc/hostname"):
            path = extract / rel
            if path.exists():
                check_text(f"{iso.name}:/{rel}", path.read_text(errors="replace"), failures)


def check_root(root, failures):
    root = Path(root)
    for rel in ("usr/lib/os-release", "etc/os-release", "etc/issue"):
        path = root / rel
        if not path.exists():
            failures.append(f"{rel} missing")
        else:
            check_text(rel, path.read_text(errors="replace"), failures)

    entries = root / "boot" / "loader" / "entries"
    if not entries.exists():
        failures.append("no boot loader entries under /boot/loader/entries")
    for entry in sorted(entries.glob("*.conf")) if entries.exists() else []:
        text = entry.read_text(errors="replace")
        check_text(entry.name, text, failures)
        if not all(opt in text for opt in SPLASH_OPTIONS):
            failures.append(f"{entry.name}: no splash options")

    mkinitcpio = root / "etc" / "mkinitcpio.conf"
    if mkinitcpio.exists() and "plymouth" not in mkinitcpio.read_text():
        failures.append("mkinitcpio.conf lacks the plymouth hook")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--iso", type=Path)
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()

    failures = []
    check_sources(failures)
    if args.iso:
        check_iso(args.iso, failures)
    if args.root:
        check_root(args.root, failures)

    if failures:
        print("branding: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("branding: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
