"""Install matrix for bingux-installer on the live ISO.

For each case: boot the live ISO with a blank 10 GiB virtio disk, write a plan through the serial
shell, run `bingux-installer --json probe` and `install`, power off, then boot the installed disk
alone and check the installed system (root filesystem, LUKS, the created user, the greetd greeter).

BIOS with encryption installs an unencrypted ext4 /boot and a LUKS2 root; GRUB unlocks nothing and the initramfs prompts once.

The serial getty autologins root (GETTY_DROPIN in vmdrive/vm.py), so the boot checks run as root;
the created user is checked with runuser and wheel membership, not a password login.

Usage: python3 run.py --iso PATH [--case NAME ...] [--out DIR] [--disk-dir DIR]
Exit status is 0 only when every case passes. Artifacts: OUT/<case>/{install,boot}/ and results.json.
"""

import argparse
import base64
import json
import re
import shlex
import shutil
import subprocess
import tempfile
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "vm"))

from vmdrive.checks import Checks  # noqa: E402
from vmdrive.vm import LiveVm  # noqa: E402
from fixtures import CHECK_MANIFEST, build_other_os_disk, build_qcow2, verify_ntfs_manifest  # noqa: E402

DISK_BYTES = 10 * 1024**3
DISK = "/dev/vda"
USER = "tester"
USER_PASSWORD = "tester-pass-1"
ROOT_PASSWORD = "root-pass-456"
SWAP_BYTES = 2 * 1024**3
LUKS_PASSPHRASE = "luks-pass-123"
DEFAULT_OUT = HERE.parents[2] / "out" / "installer-backend" / "matrix"
DEFAULT_DISK_DIR = Path.home() / ".cache" / "bingux-installer-matrix"
OVERLAY_LABEL = "BINGUXOV"  # volume id of the installer overlay CD
OVERLAY_MOUNT = "/mnt/ov"
OVERLAY_SCRATCH = Path("/tmp") / "bingux-installer-overlay"

# name: (firmware, filesystem, encrypted, extra plan fields)
CASES = {
    "uefi-ext4": ("uefi", "ext4", False, {}),
    "uefi-btrfs": ("uefi", "btrfs", False, {}),
    "uefi-luks-ext4": ("uefi", "ext4", True, {}),
    "bios-ext4": ("bios", "ext4", False, {}),
    "bios-btrfs": ("bios", "btrfs", False, {}),
    "bios-luks-ext4": ("bios", "ext4", True, {}),
    # xfs root, timesyncd off, root password set
    "uefi-xfs": ("uefi", "xfs", False, {"ntp": False, "root_password": ROOT_PASSWORD}),
    # swap file on ext4, timesyncd on (default), root locked (default)
    "uefi-ext4-swapfile": ("uefi", "ext4", False, {"swap": "file", "swap_bytes": SWAP_BYTES}),
    # zram swap on btrfs
    "uefi-btrfs-zram": ("uefi", "btrfs", False, {"swap": "zram"}),
    # encrypted swap partition on LUKS root
    "uefi-luks-ext4-swappart": ("uefi", "ext4", True, {"swap": "partition", "swap_bytes": SWAP_BYTES}),
    # first user starts the Gnoblin session at boot (greetd initial_session); greeter kept as default
    "uefi-ext4-autologin": ("uefi", "ext4", False, {"autologin": USER}),
    # manual: reuse the ESP, format a home partition, create the root in the free space (see PREPARE)
    "uefi-manual": ("uefi", "ext4", False, {"storage": {"mode": "manual", "partitions": [
        {"device": DISK + "1", "action": "reuse", "role": "esp"},
        {"device": DISK + "2", "action": "format", "role": "home", "fs": "ext4"},
        {"device": DISK + "3", "action": "create", "role": "root", "size_bytes": 6 * 1024**3},
    ]}}),
    # another OS (ext4, 4 GiB, manifest-checked) on a 10 GiB disk: install into the unallocated rest
    "uefi-free-space": ("uefi", "ext4", False, {"storage": {"mode": "free_space"}}),
    "bios-free-space": ("bios", "ext4", False, {"storage": {"mode": "free_space"}}),
    # another OS (ext4, 6 GiB) shrunk to 4 GiB to make room for the root
    "uefi-alongside-ext4": ("uefi", "ext4", False, {"storage": {"mode": "alongside", "shrink": {
        "partition": DISK + "1", "size_bytes": 4 * 1024**3}}}),
    # another OS (NTFS, 6 GiB, manifest-checked) shrunk to 4 GiB with ntfsresize, then the root is created
    "uefi-alongside-ntfs": ("uefi", "ext4", False, {"storage": {"mode": "alongside", "shrink": {
        "partition": DISK + "1", "size_bytes": 4 * 1024**3}}}),
    # BIOS manual: reuse the bios_boot partition, format a home partition, create the root in free space
    "manual-bios": ("bios", "ext4", False, {"storage": {"mode": "manual", "partitions": [
        {"device": DISK + "1", "action": "reuse", "role": "bios_boot"},
        {"device": DISK + "2", "action": "format", "role": "home", "fs": "ext4"},
        {"device": DISK + "3", "action": "create", "role": "root", "size_bytes": 6 * 1024**3},
    ]}}),
    # UEFI manual with an encrypted root: reuse the ESP, create the LUKS root in free space (no home)
    "manual-luks": ("uefi", "ext4", True, {"storage": {"mode": "manual", "partitions": [
        {"device": DISK + "1", "action": "reuse", "role": "esp"},
        {"device": DISK + "2", "action": "create", "role": "root", "size_bytes": 6 * 1024**3},
    ]}}),
    # only ~0.5 GiB unallocated: free_space must refuse and leave the other OS untouched
    "uefi-free-space-refused": ("uefi", "ext4", False, {"storage": {"mode": "free_space"}}),
}

# Cases whose disk starts as another OS: name -> (other-OS partition bytes, filesystem). Built on the host.
FIXTURES = {
    "uefi-free-space": (4 * 1024**3, "ext4"),
    "bios-free-space": (4 * 1024**3, "ext4"),
    "uefi-alongside-ext4": (6 * 1024**3, "ext4"),
    "uefi-alongside-ntfs": (6 * 1024**3, "ntfs"),
    "uefi-free-space-refused": (int(9.5 * 1024**3), "ext4"),
}
# Cases that must refuse at install time: the install result is not ok and nothing is booted.
REFUSALS = {"uefi-free-space-refused"}

# Guest disk layout a case needs before the probe: the matrix blank disk has no partition table.
PREPARE = {
    "uefi-manual": (f"printf '%s\\n' 'label: gpt' ',512MiB,U' ',2GiB,L' | sfdisk --wipe always {DISK} && "
                    f"udevadm settle && mkfs.vfat -F 32 -n BINGUX_ESP {DISK}1"),
    # BIOS boot partition (GUID 21686148-...) then a home partition for the manual case to format
    "manual-bios": (f"printf '%s\\n' 'label: gpt' ',1MiB,21686148-6449-6E6F-744E-656564454649' ',2GiB,L' | "
                    f"sfdisk --wipe always {DISK} && udevadm settle"),
    "manual-luks": (f"printf '%s\\n' 'label: gpt' ',512MiB,U' | sfdisk --wipe always {DISK} && "
                    f"udevadm settle && mkfs.vfat -F 32 -n BINGUX_ESP {DISK}1"),
}


def make_plan(firmware, filesystem, encrypted, extra=None):
    encryption = {"enabled": encrypted}
    if encrypted:
        encryption["passphrase"] = LUKS_PASSPHRASE
    plan = {
        "version": 2,
        "disk": DISK,
        "confirm_erase": DISK,
        "firmware": firmware,
        "filesystem": filesystem,
        "encryption": encryption,
        "other_os": "refuse",
        "hostname": "bingux-test",
        "timezone": "Europe/London",
        "locale": "en_GB.UTF-8",
        "keymap": "uk",
        "users": [{"name": USER, "password": USER_PASSWORD, "admin": True, "full_name": "Tester"}],
        "kernel_args": ["console=ttyS0"],
    }
    plan.update(extra or {})
    mode = plan.get("storage", {}).get("mode", "erase")
    if mode not in ("erase", "manual"):
        del plan["confirm_erase"]  # the plan schema allows confirm_erase only where the disk may be erased
    if mode != "erase":
        del plan["other_os"]  # the plan schema allows other_os only in erase mode
    return plan

ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[()][0-9A-Za-z]|\x1b[=>78]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def clean_output(text):
    """Guest output without terminal escapes.

    The shell's prompt integration prints OSC sequences after each command, so the next
    command's output starts with the tail of the previous one (`...;exit=success ESC \\`).
    """
    text = re.sub(r"^[^\n]*?\x1b\\", "", text, count=1)
    return ANSI.sub("", text)


class GuestVm:
    """LiveVm proxy whose run() returns output with terminal escapes removed."""

    def __init__(self, vm):
        self._vm = vm

    def run(self, command, **kwargs):
        rc, out = self._vm.run(command, **kwargs)
        return rc, clean_output(out)

    def __getattr__(self, name):
        return getattr(self._vm, name)


def last_json(text):
    """The last line of `text` that parses as a JSON object, or None."""
    for line in reversed(text.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                value = json.loads(line)
            except ValueError:
                continue
            if isinstance(value, dict):
                return value
    return None


def build_overlay(src, scratch):
    """Pack src/bingux_installer into a data CD labelled OVERLAY_LABEL, so the guest runs this copy."""
    staging = scratch / "root"
    shutil.rmtree(scratch, ignore_errors=True)
    shutil.copytree(src / "bingux_installer", staging / "bingux_installer",
                    ignore=shutil.ignore_patterns("__pycache__"))
    iso = scratch / "overlay.iso"
    subprocess.run(["xorriso", "-as", "mkisofs", "-quiet", "-R", "-J", "-V", OVERLAY_LABEL,
                    "-o", str(iso), str(staging)], check=True)
    return iso


def install_phase(vm, checks, name, firmware, filesystem, encrypted, extra, overlay=False, refuse=False):
    """Boot the live ISO, run probe and install. True when the install reports ok."""
    prefix = f"PYTHONPATH={OVERLAY_MOUNT} " if overlay else ""
    if not checks.expect(vm.wait_for_shell(timeout=600), f"{name}: live root shell up"):
        return False
    if overlay:
        rc, _ = vm.run(f"mkdir -p {OVERLAY_MOUNT} && mount -o ro LABEL={OVERLAY_LABEL} {OVERLAY_MOUNT}",
                       timeout=60)
        checks.expect(rc == 0, f"{name}: installer overlay mounted", f"rc={rc}")
    if overlay:
        # The install runs in the socket-activated daemon, not the CLI: give the daemon the overlay.
        dropin = base64.b64encode(f"[Service]\nEnvironment=PYTHONPATH={OVERLAY_MOUNT}\n".encode()).decode()
        rc, _ = vm.run(f"mkdir -p /run/systemd/system/bingux-installer.service.d && printf %s {dropin} | "
                       "base64 -d > /run/systemd/system/bingux-installer.service.d/overlay.conf && "
                       "systemctl daemon-reload && systemctl restart bingux-installer.service", timeout=120)
        checks.expect(rc == 0, f"{name}: daemon restarted on the overlay", f"rc={rc}")
    if name in PREPARE:
        rc, _ = vm.run(PREPARE[name], timeout=180)
        checks.expect(rc == 0, f"{name}: guest disk prepared", f"rc={rc}")
    payload = base64.b64encode(json.dumps(make_plan(firmware, filesystem, encrypted, extra)).encode()).decode()
    rc, _ = vm.run(f"printf %s {payload} | base64 -d > /tmp/plan.json", timeout=60)
    checks.expect(rc == 0, f"{name}: plan written to the guest", f"rc={rc}")

    rc, probe_out = vm.run(f"{prefix}bingux-installer --json probe", timeout=180)
    probe = last_json(probe_out) or {}
    data = probe.get("data", {})
    target = next((d for d in data.get("disks", []) if d.get("path") == DISK), None)
    checks.expect(
        probe.get("event") == "result" and data.get("firmware") == firmware
        and target is not None and target.get("is_live_medium") is False,
        f"{name}: probe reports {firmware} firmware and {DISK} as an install target",
        f"rc={rc} probe={probe}",
    )

    rc, install_out = vm.run(f"{prefix}bingux-installer --json install --plan /tmp/plan.json", timeout=2400)
    result = last_json(install_out) or {}
    data = result.get("data", {})
    if refuse:
        checks.expect(result.get("ok") is False, f"{name}: install refused", f"rc={rc} result={result}")
        return False
    ok = result.get("event") == "result" and result.get("ok") is True
    checks.expect(ok, f"{name}: install result ok", f"rc={rc} result={result}")
    checks.expect(data.get("filesystem") == filesystem and data.get("firmware") == firmware,
                  f"{name}: install reports {firmware} and {filesystem}", f"data={data}")
    return ok


def login_shell(vm, timeout):
    """Log in as USER on the serial getty; the installed system has no autologin. True when a shell is up."""
    if not vm.wait_for("login:", timeout=timeout):
        return False
    vm.send(USER + "\n")
    if not vm.wait_for("assword:", timeout=60):
        return False
    vm.send(USER_PASSWORD + "\n")
    return vm.wait_for_shell(timeout=300, boot_delay=0)


def check_features(vm, checks, name, encrypted, extra):
    """Options beyond the base install: ntp, root password, swap kind."""
    rc, ntp = vm.run("systemctl is-enabled systemd-timesyncd.service; true", timeout=60)
    want_ntp = "enabled" if extra.get("ntp", True) else "disabled"
    checks.expect(ntp.strip().splitlines()[-1:] == [want_ntp], f"{name}: timesyncd is {want_ntp}",
                  f"out={ntp!r}")
    rc, root_state = vm.run(f"printf %s {USER_PASSWORD} | sudo -S -p '' passwd -S root 2>/dev/null",
                            timeout=60)
    want_root = "P" if extra.get("root_password") else "L"
    status = [line.split() for line in root_state.splitlines() if line.startswith("root ")]
    checks.expect(rc == 0 and len(status) == 1 and status[0][1:2] == [want_root],
                  f"{name}: root password {'set' if want_root == 'P' else 'locked'}",
                  f"rc={rc} out={root_state!r}")
    swap = extra.get("swap", "none")
    if swap == "none":
        return
    if swap == "file":
        want = "/swapfile"
    elif swap == "zram":
        want = "/dev/zram0"
    else:
        want = "/dev/mapper/bingux-swap" if encrypted else f"{DISK}2"
    # dm devices show up as /dev/dm-N in swapon, so compare resolved device names.
    rc, active = vm.run(f"readlink -f {want}; swapon --noheadings --show=NAME | xargs -r -n1 readlink -f",
                        timeout=60)
    devices = active.split()
    checks.expect(rc == 0 and len(devices) > 1 and devices[0] in devices[1:],
                  f"{name}: {swap} swap is active",
                  f"rc={rc} active={active!r}")


def boot_phase(vm, checks, name, firmware, filesystem, encrypted, extra, out):
    """Boot the installed disk alone and check the installed system."""
    if encrypted:
        # Must be called right after start(): the prompt is matched from this point on.
        if checks.expect(vm.wait_for("assphrase", timeout=600), f"{name}: LUKS passphrase prompt"):
            vm.send(LUKS_PASSPHRASE + "\n")
    if not checks.expect(login_shell(vm, timeout=900), f"{name}: installed system login as {USER}"):
        return

    rc, who = vm.run("id -un", timeout=120)
    checks.expect(rc == 0 and who.strip() == USER, f"{name}: created user runs commands", f"rc={rc} out={who!r}")
    rc, groups = vm.run(f"id -nG {USER}", timeout=60)
    checks.expect(rc == 0 and "wheel" in groups.split(), f"{name}: admin user is in wheel", f"out={groups!r}")
    if extra.get("autologin"):
        rc, greetd = vm.run("grep -c '^\\[initial_session\\]' /etc/greetd/config.toml; "
                            f"grep -c 'user = \"{USER}\"' /etc/greetd/config.toml; "
                            "grep -c '^\\[default_session\\]' /etc/greetd/config.toml", timeout=60)
        checks.expect(greetd.split()[-3:] == ["1", "1", "1"],
                      f"{name}: greetd starts {USER} in gnoblin and keeps the greeter",
                      f"rc={rc} out={greetd!r}")
    else:
        rc, greetd = vm.run("grep -c initial_session /etc/greetd/config.toml; "
                            "cmp -s /etc/greetd/config.toml /usr/share/bingux/greeter/greetd.toml && echo packaged",
                            timeout=60)
        checks.expect(greetd.split()[-2:] == ["0", "packaged"],
                      f"{name}: greetd config is the packaged greeter, no live session",
                      f"rc={rc} out={greetd!r}")
    rc, live_bits = vm.run("id live >/dev/null 2>&1 && echo live-user; "
                           "pacman -Q bingux-installer >/dev/null 2>&1 && echo installer-pkg; "
                           "ls /usr/local/bin/bingux-live-session /usr/local/bin/bingux-live-switch 2>/dev/null",
                           timeout=60)
    checks.expect(live_bits.strip() == "", f"{name}: no live-only account, package or scripts",
                  f"rc={rc} out={live_bits!r}")

    rc, fstype = vm.run("findmnt -n -o FSTYPE /", timeout=60)
    checks.expect(rc == 0 and fstype.strip() == filesystem, f"{name}: root is {filesystem}",
                  f"rc={rc} out={fstype!r}")
    if encrypted:
        rc, types = vm.run(f"lsblk -rn -o TYPE {DISK}", timeout=60)
        checks.expect(rc == 0 and "crypt" in types.split(), f"{name}: root sits on LUKS", f"out={types!r}")

    check_features(vm, checks, name, encrypted, extra)

    state = "unknown"
    for _ in range(24):
        rc, state_out = vm.run("systemctl is-active greetd", timeout=60)
        state = state_out.strip().splitlines()[-1] if state_out.strip() else "unknown"
        if state == "active":
            break
    checks.expect(state == "active", f"{name}: greetd greeter active", f"state={state!r}")
    if extra.get("autologin"):
        running = ""
        for _ in range(24):
            rc, running = vm.run(f"sleep 5; pgrep -u {USER} -x gnoblin; true", timeout=60)
            if running.strip():
                break
        checks.expect(running.strip() != "", f"{name}: gnoblin session running for {USER} at boot",
                      f"out={running!r}")
    rc, failed = vm.run("systemctl --failed --no-legend", timeout=60)
    (out / "failed-units.txt").write_text(failed + "\n", encoding="utf-8")
    vm.screenshot(out / "greeter.png")
    checks.expect((out / "greeter.png").stat().st_size > 0, f"{name}: greeter screenshot written")


def run_case(name, iso, out_root, disk_dir, checks, cdrom=None):
    firmware, filesystem, encrypted, extra = CASES[name]
    out = out_root / name
    disk_dir.mkdir(parents=True, exist_ok=True)
    disk = disk_dir / f"{name}.qcow2"
    disk.unlink(missing_ok=True)
    if name in FIXTURES:
        raw = disk_dir / f"{name}.raw"
        build_other_os_disk(raw, DISK_BYTES, *FIXTURES[name])
        build_qcow2(raw, disk)

    installed = False
    with LiveVm(out / "install") as vm:
        vm.start(iso=iso, disks=[(disk, DISK_BYTES)], firmware=firmware, cdrom=cdrom)
        installed = install_phase(GuestVm(vm), checks, name, firmware, filesystem, encrypted, extra,
                                  overlay=cdrom is not None, refuse=name in REFUSALS)
        if installed:
            vm.run("sync", timeout=120)
            vm.poweroff()
            checks.expect(not vm.alive(), f"{name}: live guest powered off")
    if installed:
        with LiveVm(out / "boot") as vm:
            vm.start(iso=None, disks=[(disk, DISK_BYTES)], firmware=firmware)
            boot_phase(GuestVm(vm), checks, name, firmware, filesystem, encrypted, extra, out)
            if name in FIXTURES and FIXTURES[name][1] == "ext4":
                rc, manifest = GuestVm(vm).run(
                    f"printf %s {USER_PASSWORD} | sudo -S -p '' sh -c {shlex.quote(CHECK_MANIFEST)}", timeout=120)
                checks.expect(rc == 0, f"{name}: other OS manifest verifies after install",
                              f"rc={rc} out={manifest[-300:]!r}")
            vm.poweroff()
    if installed and name in FIXTURES and FIXTURES[name][1] == "ntfs":
        # the installed guest has no NTFS driver: read the partition from the host once the guest is off
        with tempfile.TemporaryDirectory(dir=disk_dir) as scratch:
            ok, detail = verify_ntfs_manifest(disk, "qcow2", Path(scratch))
        checks.expect(ok, f"{name}: other OS manifest verifies after install (host ntfscat)", detail)
    disk.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(prog="matrix", description=__doc__.split("\n")[0])
    parser.add_argument("--iso", required=True, help="live ISO to boot as -cdrom")
    parser.add_argument("--case", action="append", choices=sorted(CASES), help="case to run (repeatable)")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="artifact root")
    parser.add_argument("--disk-dir", default=str(DEFAULT_DISK_DIR),
                        help="target disk images (large; removed after each case)")
    parser.add_argument("--overlay", help="directory holding bingux_installer/: run it instead of the ISO copy")
    args = parser.parse_args(argv)
    cdrom = build_overlay(Path(args.overlay), OVERLAY_SCRATCH) if args.overlay else None

    out_root = Path(args.out)
    out_root.mkdir(parents=True, exist_ok=True)
    overall = True
    for name in args.case or list(CASES):
        out = out_root / name
        out.mkdir(parents=True, exist_ok=True)
        checks = Checks(out)
        try:
            run_case(name, Path(args.iso), out_root, Path(args.disk_dir), checks, cdrom)
        except Exception as error:  # reported as a failed check; artifacts are kept
            checks.expect(False, f"{name}: driver completed without error", repr(error))
        finally:
            checks.save()
        overall = overall and checks.ok
    return 0 if overall else 1


if __name__ == "__main__":
    sys.exit(main())
