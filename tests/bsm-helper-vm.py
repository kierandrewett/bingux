#!/usr/bin/env python3
"""Run tests/bsm-helper-root.sh as root in the live Bingux OS image.

usage: tests/bsm-helper-vm.py [--iso ISO] [--out DIR] [--skip-build] [--keep-work] [--memory 3G]

Builds bsm-helper and its probe client on the host (release, private target
directory), stages them with the PAM services, the units and the sysusers file
from packaging/bsm into a directory, boots the live ISO with that directory
shared over virtio-9p, installs it in the guest and runs the root-only checks.
The plain image is booted (no `bingux.bsm`), so greetd keeps the display and
nothing here depends on the compositor or on hosted Gnoblin.

Limits: the binaries are built against the Fedora host's glibc and run on the
Arch image, which must provide the same sonames (libpam.so.0, libsystemd.so.0);
the guest check is the `ldd` output printed before the run. The live root is an
overlay: nothing persists.

Artifacts in --out (default distro/out/bsm-test): helper-root.txt (the script's
output), helper-root-serial.log. Exit status: 0 all checks passed, 1 a check
failed, 2 the VM or the setup failed (nothing was checked).
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import distro_vm  # noqa: E402
from distro_vm import ROOT, Vm, find_iso, missing_tools  # noqa: E402

TARGET = Path(os.environ.get("BSM_HELPER_TARGET", "/tmp/bsm-target/helper-release"))
STAGE = Path(os.environ.get("BSM_HELPER_STAGE", "/tmp/bsm-vm/helper-stage"))


class ShareVm(Vm):
    """A Vm that shares STAGE with the guest as the 9p tag `bsmhelper`."""

    def __init__(self, work, share, **kwargs):
        super().__init__(work, serial=True, **kwargs)
        self.share = share

    def start(self):
        original = distro_vm.display_args
        share = self.share

        def args():
            base = ["-device", "virtio-vga", "-display", "none"]
            return base + [
                "-virtfs",
                f"local,path={share},mount_tag=bsmhelper,security_model=none,readonly=on,id=bsmhelper",
            ]

        distro_vm.display_args = args
        try:
            super().start()
        finally:
            distro_vm.display_args = original


def build():
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    cargo = os.environ.get("CARGO") or str(Path.home() / ".cargo/bin/cargo")
    if not os.access(cargo, os.X_OK):
        cargo = "cargo"
    subprocess.run(
        [cargo, "build", "--release", "--bins", "--examples"],
        cwd=ROOT / "bsm/helper",
        env=env,
        check=True,
    )


def stage():
    if STAGE.exists():
        shutil.rmtree(STAGE)
    files = {
        TARGET / "release/bsm-helper": "root/usr/libexec/bsm/bsm-helper",
        TARGET / "release/examples/bsm-helper-probe": "root/usr/libexec/bsm/bsm-helper-probe",
        ROOT / "packaging/bsm/pam/bsm-login": "root/etc/pam.d/bsm-login",
        ROOT / "packaging/bsm/pam/bsm-unlock": "root/etc/pam.d/bsm-unlock",
        ROOT / "packaging/bsm/systemd/bsm-helper.socket": "root/usr/lib/systemd/system/bsm-helper.socket",
        ROOT / "packaging/bsm/systemd/bsm-helper.service": "root/usr/lib/systemd/system/bsm-helper.service",
        ROOT / "packaging/bsm/sysusers/bsm.conf": "root/usr/lib/sysusers.d/bsm.conf",
        ROOT / "tests/bsm-helper-root.sh": "bsm-helper-root.sh",
    }
    for source, destination in files.items():
        target = STAGE / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    return STAGE


GUEST_SETUP = r"""
mkdir -p /mnt/bsmhelper
modprobe 9pnet_virtio 2>/dev/null; modprobe 9p 2>/dev/null
mount -t 9p -o trans=virtio,version=9p2000.L,ro bsmhelper /mnt/bsmhelper || echo MOUNT_FAILED
tar -C /mnt/bsmhelper/root -cf - . | tar -C / -xpf - --no-same-owner
chmod 755 /usr/libexec/bsm/bsm-helper /usr/libexec/bsm/bsm-helper-probe
echo "--- ldd"; ldd /usr/libexec/bsm/bsm-helper | grep -i 'not found' || echo "all libraries resolved"
echo "--- guest"; . /etc/os-release; echo "$PRETTY_NAME $(uname -r) systemd $(systemctl --version | head -n 1)"
echo "--- pam"; ls /etc/pam.d | tr '\n' ' '; echo; grep -c . /etc/pam.d/system-auth
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--iso")
    parser.add_argument("--out", default=os.environ.get("BINGUX_DISTRO_OUT", str(ROOT / "distro/out/bsm-test")))
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--keep-work", action="store_true")
    parser.add_argument("--memory", default="3G")
    args = parser.parse_args()

    missing = missing_tools("qemu-system-x86_64", "cargo")
    iso = find_iso(args.iso)
    if missing or iso is None or not iso.exists():
        print(f"cannot run: missing {missing or 'ISO'}")
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if not args.skip_build:
        build()
    stage()

    work = Path(tempfile.mkdtemp(prefix="bsm-helper-vm-", dir="/tmp"))
    vm = ShareVm(work, STAGE, iso=iso, memory=args.memory, record=work / "record")
    status = 2
    try:
        vm.start()
        if not vm.start_shell(timeout=420):
            print("the guest console never became usable")
            return 2
        setup = vm.run(GUEST_SETUP, timeout=120)
        print(setup)
        if "MOUNT_FAILED" in setup:
            print("9p share could not be mounted; nothing was checked")
            return 2
        started = time.monotonic()
        text = vm.run("sh /mnt/bsmhelper/bsm-helper-root.sh 2>&1", timeout=900)
        print(text)
        (out / "helper-root.txt").write_text(setup + "\n" + text + f"\n(ran {time.monotonic() - started:.0f} s)\n")
        if "RESULT failures=0" in text and "FAIL" not in text.replace("RESULT failures=0", ""):
            status = 0
        elif "RESULT failures=" in text:
            status = 1
        else:
            print("the script did not reach its result line; treat as not run")
            status = 2
    finally:
        try:
            serial = getattr(vm, "log", b"")
            (out / "helper-root-serial.log").write_bytes(serial[-400000:])
        except Exception:  # noqa: BLE001
            pass
        vm.stop()
        if not args.keep_work:
            shutil.rmtree(work, ignore_errors=True)
    return status


if __name__ == "__main__":
    sys.exit(main())
