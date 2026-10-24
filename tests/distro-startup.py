#!/usr/bin/env python3
"""Check the live image's boot settings that set its start-up time.

usage: tests/distro-startup.py [--iso ISO]

Always checks the sources in distro/profile. With --iso it also extracts the
boot menus from a built image (xorriso) and checks what the image boots with:
archiso's releng profile supplies any file that distro/profile does not
replace, so only the image shows what a machine sees.

- Boot menus: the UEFI menu (systemd-boot, loader.conf) and the BIOS menu
  (syslinux, archiso_sys.cfg) wait at most 5 s, and syslinux's DEFAULT names an
  entry that exists. A menu always waits its full timeout. GRUB boots the image
  only through loopback.cfg; its timeout is checked as well.
- Every live boot entry passes systemd.condition_needs_update=0. The live root
  is a fresh overlay on each boot, so /etc/.updated never exists and
  ldconfig.service rebuilt the linker cache before sysinit.target on every
  boot (20.7 s and 20.9 s in two QEMU boots of the c0c2c3a image). pacman
  already built that cache, the hwdb, the journal catalogue and the system
  accounts when the image was made.

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
MAX_MENU_SECONDS = 5
LIVE_OPTION = "systemd.condition_needs_update=0"

# Relative paths in distro/profile and in the built image.
LOADER_CONF = ("efiboot/loader/loader.conf", "loader/loader.conf")
ENTRIES = ("efiboot/loader/entries", "loader/entries")
SYSLINUX_MENU = ("syslinux/archiso_sys.cfg", "boot/syslinux/archiso_sys.cfg")
SYSLINUX_ENTRIES = ("syslinux/archiso_sys-linux.cfg", "boot/syslinux/archiso_sys-linux.cfg")
GRUB_MENUS = (("grub/grub.cfg", None), ("grub/loopback.cfg", "boot/grub/loopback.cfg"))


def live_kernel_lines(text, kind):
    """Kernel command lines of the live entries: systemd-boot `options`, syslinux `APPEND`, GRUB `linux`."""
    lines = [line.strip() for line in text.splitlines()]
    if kind == "systemd-boot":
        return [line for line in lines if line.startswith("options")]
    if kind == "syslinux":
        return [line for line in lines if line.startswith("APPEND")]
    return [line for line in lines if line.startswith("linux ") and "vmlinuz-linux" in line]


def check_entries(name, text, kind, failures):
    lines = live_kernel_lines(text, kind)
    if not lines:
        failures.append(f"{name}: no live boot entry found")
    for line in lines:
        if LIVE_OPTION not in line.split():
            failures.append(f"{name}: live entry lacks {LIVE_OPTION}: {line[:80]}")


def check_loader_conf(name, text, failures):
    match = re.search(r"^\s*timeout\s+(\d+)\s*$", text, re.MULTILINE)
    if not match:
        failures.append(f"{name}: no timeout")
    elif int(match.group(1)) > MAX_MENU_SECONDS:
        failures.append(f"{name}: menu waits {match.group(1)} s, more than {MAX_MENU_SECONDS} s")


def check_syslinux(name, menu, entries, failures):
    timeout = re.search(r"^\s*TIMEOUT\s+(\d+)\s*$", menu, re.MULTILINE)
    if not timeout:
        failures.append(f"{name}: no TIMEOUT")
    elif int(timeout.group(1)) > MAX_MENU_SECONDS * 10:
        failures.append(f"{name}: menu waits {int(timeout.group(1)) / 10:g} s, more than {MAX_MENU_SECONDS} s")
    default = re.search(r"^\s*DEFAULT\s+(\S+)\s*$", menu, re.MULTILINE)
    labels = re.findall(r"^\s*LABEL\s+(\S+)\s*$", entries, re.MULTILINE)
    if not default:
        failures.append(f"{name}: no DEFAULT")
    elif default.group(1) not in labels:
        failures.append(f"{name}: DEFAULT {default.group(1)} is not one of the entries {labels}")


def check_grub(name, text, failures):
    match = re.search(r"^\s*timeout=(\d+)\s*$", text, re.MULTILINE)
    if not match:
        failures.append(f"{name}: no timeout")
    elif int(match.group(1)) > MAX_MENU_SECONDS:
        failures.append(f"{name}: menu waits {match.group(1)} s, more than {MAX_MENU_SECONDS} s")
    check_entries(name, text, "grub", failures)


def check_tree(root, index, label, failures):
    """Check one tree: index 0 is distro/profile, index 1 is an extracted image."""
    root = Path(root)

    def read(rel):
        path = root / rel
        if not path.is_file():
            failures.append(f"{label}: missing {rel}")
            return None
        return path.read_text(errors="replace")

    text = read(LOADER_CONF[index])
    if text is not None:
        check_loader_conf(f"{label}:{LOADER_CONF[index]}", text, failures)
    entries = sorted((root / ENTRIES[index]).glob("*-linux.conf"))
    if not entries:
        failures.append(f"{label}: no live entries in {ENTRIES[index]}")
    for entry in entries:
        check_entries(f"{label}:{entry.relative_to(root)}", entry.read_text(errors="replace"), "systemd-boot", failures)
    menu, linux = read(SYSLINUX_MENU[index]), read(SYSLINUX_ENTRIES[index])
    if menu is not None and linux is not None:
        check_syslinux(f"{label}:{SYSLINUX_MENU[index]}", menu, linux, failures)
        check_entries(f"{label}:{SYSLINUX_ENTRIES[index]}", linux, "syslinux", failures)
    for paths in GRUB_MENUS:
        if paths[index] is not None:
            text = read(paths[index])
            if text is not None:
                check_grub(f"{label}:{paths[index]}", text, failures)


def check_iso(iso, failures):
    with tempfile.TemporaryDirectory(prefix="startup-iso.") as tmp:
        out = Path(tmp)
        for path in ("/loader", "/boot/syslinux", "/boot/grub"):
            # auto_chmod_on: the image's directories are read-only, and /boot holds two of them.
            subprocess.run(
                ["xorriso", "-osirrox", "on:auto_chmod_on", "-indev", str(iso), "-extract", path, str(out / path[1:])],
                check=False,
                capture_output=True,
            )
        check_tree(out, 1, iso.name, failures)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--iso", type=Path)
    args = parser.parse_args()

    failures = []
    check_tree(PROFILE, 0, "distro/profile", failures)
    if args.iso:
        check_iso(args.iso, failures)

    if failures:
        print("startup: FAIL")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("startup: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
