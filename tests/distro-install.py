#!/usr/bin/env python3
"""Install Bingux OS through the graphical installer, then restart into the result.

usage: tests/distro-install.py [uefi|uefi-luks|bios] [ISO]

uefi is the default variant. ISO defaults to the newest distro/out/bingux-os-*.iso.

1. Boot the live image on a blank 20 GiB disk (OVMF for the uefi variants,
   SeaBIOS for bios) with the screen recorded. The live session lands in the
   installer.
2. Drive every installer page from the frames (tests/distro_installer_driver.py):
   welcome, keyboard, network (skipped), time zone (a city on the map), computer
   name (kept), disk (the blank disk), storage (LUKS2 on uefi-luks), account,
   summary (confirm the erase), installation, finish.
3. Check the installed disk over the serial console: the installed packages, no
   installer package or files, greetd without initial_session, no live account,
   and branding in the installed boot files (tests/distro-branding.py --root).
4. Press Restart with the medium still in the drive. After the guest resets
   itself, the medium is removed and the firmware reset, so the same VM boots
   the installed disk alone: unlock it at the passphrase prompt (uefi-luks),
   wait for the greeter, sign in and check that the desktop paints.

Artifacts land in distro/out/install-test-<variant>/: checks.json, report.txt,
timeline.json, serial.log, pages/*.png (one per installer page), live/ (flow.mp4,
timeline.json, frame-events.jsonl), and installed-*.png.
Scratch space is under ~/.cache/bingux-e2e.
"""

import base64
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

from distro_capture import greeter_shown, session_painted
from distro_installer_driver import (
    Installer,
    Pointer,
    error_page_up,
    lowest_word,
    page_title,
    text_on_screen,
    zone_on_label,
)
from distro_vm import ROOT, Vm, find_iso, missing_tools

USER = "tester"
PASSWORD = "bingux-test-2026"
PASSPHRASE = "bingux-luks-2026"
# Diagnostic runs set this to send the installed boot's kernel and systemd log to the serial console.
DEBUG_CONSOLE_ENV = "BINGUX_INSTALL_DEBUG_CONSOLE"
DEBUG_CONSOLE_ARGS = "console=ttyS0,115200 systemd.journald.forward_to_console=1 systemd.log_target=console systemd.log_level=info"
CITY_ZONE = "Europe/Berlin"
CITY_LON_LAT = (13.4, 52.5)
# Offsets (px) tried in turn when a click on the map picks another zone.
MAP_NUDGES = [(0, 0), (4, 0), (-4, 0), (0, 4), (0, -4)]
MAP_GAP = 12  # px between the bottom of the time zone map and the "Time zone:" label
ZONE_NAMES = [entry["zone"] for entry in json.loads((ROOT / "distro/installer/shell/assets/timezones.json").read_text())]
VARIANTS = {
    "uefi": {"uefi": True, "luks": False},
    "uefi-luks": {"uefi": True, "luks": True},
    "bios": {"uefi": False, "luks": False},
}
INSTALL_TIMEOUT = 1500  # seconds for the payload copy; the error page ends the wait early
ROOT_PART = "/dev/vda2"  # root (or the LUKS container) on every layout; ESP is /dev/vda1 on uefi


class Fail(Exception):
    """The installer did not behave as the page flow expects."""


def slug(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


class Timeline:
    def __init__(self, vm):
        self.vm = vm
        self.events = []

    def mark(self, event):
        t = round(time.monotonic() - self.vm.t0, 2)
        self.events.append({"t": t, "event": event})
        self.vm.recorder.mark(event)
        print(f"[{t:8.1f}s] {event}", flush=True)


def wait_for_text(vm, phrase, timeout, poll=2.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = vm.frame()
        if frame is not None and text_on_screen(frame, phrase) is not None:
            return True
        time.sleep(poll)
    return False


def probe(vm, command, timeout=120):
    """Run a shell command on the console and return its output.

    The console echoes the typed line, which still contains `$(`; only the
    expanded `RES_<value>_END` is kept.
    """
    output = vm.run(f'echo "RES_$({command})_END"', timeout)
    values = [m for m in re.findall(r"RES_(.*?)_END", output, re.S) if "$" not in m]
    return values[-1].strip() if values else None


DIAGNOSTICS = {
    "processes": "ps -eo pid,user,args | grep -E 'gnoblin|bingux|greetd|qml' | grep -v grep",
    "journal": "journalctl -b --no-pager 2>&1 | grep -iE 'installer|gnoblin|qml|greetd|live-session|mutter' | tail -n 150",
    "live-state": "for f in /run/user/*/bingux-live/*; do echo $f; cat $f; done 2>/dev/null",
    "ui-files": "ls -l /usr/bin/bingux-installer-ui /usr/bin/bingux-shell /usr/lib/bingux/bingux-installer.sxa 2>&1",
    "ui-shell-dir": "/usr/bin/bingux-shell --shell-dir --archive /usr/lib/bingux/bingux-installer.sxa 2>&1 | tail -n 20",
    "socket": "systemctl status bingux-installer.socket --no-pager 2>&1 | head -n 12",
    "live-user-journal": "journalctl --user -b --no-pager 2>&1 | tail -n 80",
}


def collect_diagnostics(vm, out):
    """Record the live session's processes, journal and loop state when the installer never appears."""
    sections = []
    for name, command in DIAGNOSTICS.items():
        try:
            value = probe(vm, command, 60)
        except Exception as error:  # the console may be wedged; keep what we can
            value = f"probe failed: {error}"
        sections.append(f"== {name}\n{value}")
    (out / "diagnostics.txt").write_text("\n".join(sections) + "\n")



def pick_city(inst, timeline, checks):
    """Click Berlin on the time zone map and check that the page's label names it.

    The map is the first item of the page column, so its width is the column
    width (the column starts at the left edge of the hint text and the card is
    centred), its height is width * 145 / 360, and it sits just above the
    "Time zone:" label with the theme gap in between. The installer picks the
    nearest zone to the click, so each click is nudged until the label names it.
    """
    frame = inst.frame()
    label = lowest_word(frame, lambda w: w.text.lower().startswith("zone"))  # the title "Time zone" is higher up
    hint = lowest_word(frame, lambda w: w.text == "Click")
    if label is None or hint is None:
        raise Fail("time zone label or map hint not found")
    body_left = hint.x0
    width = 2 * (frame.shape[1] / 2 - body_left)
    height = width * 145 / 360
    map_top = label.y0 - MAP_GAP - height
    lon, lat = CITY_LON_LAT
    x = body_left + (lon + 180) / 360 * width
    y = map_top + (85 - lat) / 145 * height
    picked = None
    for dx, dy in MAP_NUDGES:
        timeline.mark(f"clicking the time zone map at {x + dx:.0f},{y + dy:.0f} for {CITY_ZONE}")
        inst.pointer.click(x + dx, y + dy)
        time.sleep(1.5)
        picked = zone_on_label(inst.frame(), ZONE_NAMES)
        if picked == CITY_ZONE:
            break
    timeline.mark(f"time zone label reads {picked!r}")
    checks[f"time zone map click picks {CITY_ZONE}"] = picked == CITY_ZONE


def storage_page(inst, variant, checks):
    frame = inst.frame()
    if not variant["uefi"]:
        checks["bios storage page explains why encryption is unavailable"] = (
            text_on_screen(frame, "Encryption needs") is not None
        )
    if variant["luks"]:
        if not inst.click_switch("Encrypt the system disk"):
            raise Fail("encryption switch not found")
        time.sleep(1)
        if not inst.type_into("Passphrase", PASSPHRASE):
            raise Fail("passphrase field not found")
        if not inst.type_into("Confirm passphrase", PASSPHRASE):
            raise Fail("confirm passphrase field not found")
        time.sleep(1)
    inst.continue_page()


def account_page(inst):
    for caption, text, clear in (
        ("Full name", "Test User", False),
        ("Username", USER, True),
        ("Password", PASSWORD, False),
        ("Confirm password", PASSWORD, False),
    ):
        if not inst.type_into(caption, text, clear=clear):
            raise Fail(f"field '{caption}' not found")
        time.sleep(0.5)
    time.sleep(1)
    inst.continue_page()


def confirm_erase(inst, timeline):
    # The label wraps onto two lines, and OCR reads its capital I as a bar, so the phrase starts at "understand".
    if not inst.click_switch("understand that everything", last_line="will be erased"):
        raise Fail("erase confirmation not found")
    time.sleep(1)
    # The button enables a moment after the switch; a click that lands early does nothing, so click again
    # until the page leaves "Ready to install".
    for _ in range(3):
        if not inst.click_text("Erase and install"):
            raise Fail("'Erase and install' button not found")
        time.sleep(4)
        if page_title(inst.frame()) != "Ready to install":
            return
        timeline.mark("Erase and install did not start the install; clicking again")
    raise Fail("Erase and install did not start the install")


def wait_page(inst, title, timeout, poll):
    """Wait for `title`; stop early on the error page. Returns True when the page is up."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        frame = inst.frame()
        if frame is not None:
            on_screen = page_title(frame)
            if on_screen == title:
                return True
            if on_screen is None and error_page_up(frame):
                return False
        time.sleep(poll)
    return False


def run_steps(inst, vm, variant, out, checks, timeline):
    """Walk the wizard. Each entry: page title, action run once the page is up."""
    pages = out / "pages"
    pages.mkdir(parents=True, exist_ok=True)
    steps = [
        ("Keyboard", inst.continue_page),
        ("Network", inst.continue_page),
        ("Time zone", lambda: (pick_city(inst, timeline, checks), inst.continue_page())),
        ("Computer name", inst.continue_page),
        ("Disk", lambda: (inst.click_text("/dev/vda"), time.sleep(1), inst.continue_page())),
        ("Storage", lambda: storage_page(inst, variant, checks)),
        ("Your account", lambda: account_page(inst)),
        ("Ready to install", lambda: confirm_erase(inst, timeline)),
        ("Installing Bingux", None),
        ("Bingux is installed", None),
    ]
    reached = []
    for index, (title, action) in enumerate(steps, start=3):
        if title == "Installing Bingux":
            # The progress page can be too quick to catch; its absence is recorded below.
            up = wait_page(inst, title, 60, poll=1)
            checks["installation progress page was shown"] = up
            if up:
                vm.still(pages / f"{index:02d}-{slug(title)}.png", inst.frame())
                timeline.mark(f"page {title}")
            continue
        timeout = INSTALL_TIMEOUT if title == "Bingux is installed" else 180
        if not wait_page(inst, title, timeout, poll=1 if title != "Bingux is installed" else 5):
            frame = inst.frame()
            if frame is not None:
                vm.still(pages / f"{index:02d}-FAILED-{slug(title)}.png", frame)
            raise Fail(f"page '{title}' not reached; on screen: {inst.current_page()!r}")
        reached.append(title)
        timeline.mark(f"page {title}")
        vm.still(pages / f"{index:02d}-{slug(title)}.png", inst.frame())
        if action is not None:
            action()
    checks["installer reaches every page in order"] = len(reached) == len(steps) - 1
    return reached


def inspect_disk(vm, variant, out, checks, report):
    """Mount the installed disk read-only over the console and check what the installer left."""
    if variant["luks"]:
        unlock = vm.run(f"printf '%s' '{PASSPHRASE}' | cryptsetup open --key-file - {ROOT_PART} bingux-check", 120)
        report.append(f"$ cryptsetup open {ROOT_PART}\n{unlock}\n")
        root = "/dev/mapper/bingux-check"
    else:
        root = ROOT_PART
    boot = " && mount -o ro /dev/vda1 /mnt/boot" if variant["uefi"] else ""
    checks["installed disk mounts read-only"] = probe(
        vm, f"mount -o ro {root} /mnt{boot} >/dev/null 2>&1 && echo yes || echo no"
    ) == "yes"

    queries = [
        ("bingux-os package installed", "pacman -r /mnt -Qq 2>/dev/null | grep -c '^bingux-os$'", "1"),
        ("bingux-installer package absent", "pacman -r /mnt -Qq 2>/dev/null | grep -c installer", "0"),
        ("no Bingux installer files anywhere", "find /mnt -xdev -iname '*bingux*install*' 2>/dev/null | sed 's#^/mnt##' | tr '\\n' ' '", ""),
        ("no Bingux installer desktop entry or autostart", "grep -rl 'bingux-live-switch installer\\|bingux-install-live\\|bingux-installer' /mnt/etc/xdg /mnt/usr/share/wayland-sessions /mnt/usr/share/applications /mnt/etc/systemd 2>/dev/null | sed 's#^/mnt##' | tr '\\n' ' '", ""),
        ("greetd has no initial_session", "grep -c initial_session /mnt/etc/greetd/config.toml 2>/dev/null", "0"),
        ("greetd runs the Bingux greeter", "grep -c bingux-greeter-session /mnt/etc/greetd/config.toml 2>/dev/null", "1"),
        ("no live account in passwd", "grep -c '^live:' /mnt/etc/passwd 2>/dev/null", "0"),
        ("no live home directory", "ls /mnt/home | grep -c '^live$'", "0"),
        ("no live account in sudoers", "grep -rl '^live' /mnt/etc/sudoers.d 2>/dev/null | wc -l", "0"),
        ("the login account exists", f"grep -c '^{USER}:' /mnt/etc/passwd 2>/dev/null", "1"),
    ]
    for label, command, expected in queries:
        value = probe(vm, command)
        report.append(f"$ {command}\n{value}\n")
        checks[label] = value == expected

    files = "etc/os-release usr/lib/os-release etc/issue etc/mkinitcpio.conf boot/loader/entries"
    encoded = probe(vm, f"cd /mnt && tar -cf - {files} 2>/dev/null | base64 -w0") or ""
    root_copy = out / "installed-root-files"
    if root_copy.exists():
        shutil.rmtree(root_copy)
    root_copy.mkdir(parents=True)
    if encoded:
        with tarfile.open(fileobj=io.BytesIO(base64.b64decode(encoded)), mode="r:") as archive:
            archive.extractall(root_copy, filter="data")
    branding = subprocess.run(
        [sys.executable, str(ROOT / "tests/distro-branding.py"), "--root", str(root_copy)],
        capture_output=True,
        text=True,
    )
    report.append(f"$ distro-branding.py --root (installed boot files)\n{branding.stdout}{branding.stderr}\n")
    checks["no Arch Linux name in the installed boot files (branding check)"] = branding.returncode == 0

    collect_installed_diagnostics(vm, out, report)
    if os.environ.get(DEBUG_CONSOLE_ENV):
        # Diagnostic runs only: the installed boot writes its kernel and unit log to the serial console.
        vm.run(
            "mount -o remount,rw /mnt && (! mountpoint -q /mnt/boot || mount -o remount,rw /mnt/boot) "
            f"&& sed -i '/^options /s/$/ {DEBUG_CONSOLE_ARGS}/' /mnt/boot/loader/entries/bingux.conf && sync; echo done",
            120,
        )
        report.append(f"debug console: {DEBUG_CONSOLE_ARGS} appended to the installed loader entry\n")

    vm.run("umount -R /mnt >/dev/null 2>&1; cryptsetup close bingux-check >/dev/null 2>&1; echo done", 120)


INSTALLED_DIAGNOSTICS = {
    "loader entries": "cat /mnt/boot/loader/loader.conf /mnt/boot/loader/entries/*.conf 2>&1; ls -la /mnt/boot 2>&1",
    "initramfs contents (drm, virtio, plymouth, greetd, crypt)": "lsinitcpio -l /mnt/boot/initramfs-linux.img 2>&1 | grep -E 'drm|virtio|bochs|plymouth|greetd|crypt|vmwgfx|simpledrm|efifb|i915|amdgpu' | head -80",
    "mkinitcpio hooks and modules": "grep -E '^(HOOKS|MODULES|COMPRESSION)' /mnt/etc/mkinitcpio.conf 2>&1",
    "display manager link and default target": "readlink /mnt/etc/systemd/system/default.target; ls -l /mnt/etc/systemd/system/display-manager.service /mnt/etc/systemd/system/graphical.target.wants 2>&1",
    "greetd unit drop-ins": "ls -l /mnt/usr/lib/systemd/system/greetd.service.d 2>&1; cat /mnt/usr/lib/systemd/system/greetd.service.d/* 2>&1",
    "greetd config": "cat /mnt/etc/greetd/config.toml 2>&1",
    "greeter account and state": "grep -E '^(greeter|greetd)' /mnt/etc/passwd 2>&1; ls -la /mnt/var/lib/greetd /mnt/var/lib/bingux /mnt/var/lib/bingux/greeter 2>&1",
    "plymouth default theme": "readlink /mnt/usr/share/plymouth/themes/default.plymouth 2>&1; cat /mnt/etc/plymouth/plymouthd.conf 2>&1",
    "journald config and persistence": "cat /mnt/etc/systemd/journald.conf.d/* 2>&1; ls -la /mnt/var/log/journal 2>&1",
    "bingux-greeter-session": "ls -l /mnt/usr/bin/bingux-greeter-session /mnt/usr/bin/gnoblin /mnt/usr/bin/bingux-session 2>&1",
}


def collect_installed_diagnostics(vm, out, report):
    """Record the installed root's boot and greeter setup, read while it is still mounted."""
    sections = []
    for name, command in INSTALLED_DIAGNOSTICS.items():
        try:
            value = probe(vm, command, 120)
        except Exception as error:  # keep what the console still gives us
            value = f"probe failed: {error}"
        sections.append(f"== {name}\n{value}")
    (out / "installed-diagnostics.txt").write_text("\n".join(sections) + "\n")
    report.append("installed-diagnostics.txt written\n")


def boot_installed(vm, variant, out, timeline, checks):
    """After Restart, the same VM boots the installed disk: unlock, greeter, sign in."""
    checks["installed system booted with no installation medium attached"] = vm.medium() is None
    if variant["luks"]:
        prompt = wait_for_text(vm, "passphrase", 180)
        checks["passphrase prompt shown at boot"] = prompt
        if not prompt and (frame := vm.frame()) is not None:
            # Keep what the screen showed when the prompt was expected, to tell a missing prompt from a missed reading.
            vm.still(out / "luks-prompt-timeout.png", frame)
        timeline.mark("typing the disk passphrase")
        vm.type_text(PASSPHRASE)
        vm.key("ret")
    frame = vm.wait_frame(greeter_shown, attempts=120)
    checks["installed system shows the greeter"] = frame is not None
    if frame is None:
        return
    vm.still(out / "installed-greeter.png", frame)
    timeline.mark("greeter shown")
    # Keys typed while the greeter is still settling can be lost, and the greeter
    # then says "Try again": type the password again until a session paints.
    # Every frame after each attempt is kept, so a missed sign-in shows what the greeter did.
    frame = None
    for attempt in range(3):
        time.sleep(10)
        vm.type_text(PASSWORD)
        vm.key("ret")
        frame = vm.wait_frame(session_painted, attempts=60)
        if frame is not None:
            break
        last = vm.frame()
        if last is not None:
            vm.still(out / f"installed-after-login-{attempt}.png", last)
    checks["installed account signs in to a desktop"] = frame is not None
    if frame is not None:
        vm.still(out / "installed-session.png", frame)
        timeline.mark("desktop painted")


def run_variant(name, variant, iso, out, work):
    (out / "pages").mkdir(parents=True, exist_ok=True)
    out.mkdir(parents=True, exist_ok=True)
    checks = {}
    report = []
    disk = work / "disk.qcow2"
    subprocess.run(["qemu-img", "create", "-f", "qcow2", str(disk), "20G"], check=True, capture_output=True)
    vm = None
    pointer = None
    timeline = None
    inst = None
    flow_ok = False
    try:
        vm = Vm(
            work / "live",
            iso=iso,
            disk=disk,
            uefi=variant["uefi"],
            serial=True,
            vars_path=work / "OVMF_VARS.fd",
            record=out / "live",
            pointer=True,
        )
        vm.start()
        timeline = Timeline(vm)
        timeline.mark("live image powered on")
        checks["live image booted to a shell"] = vm.start_shell()
        if checks["live image booted to a shell"]:
            firmware = probe(vm, "test -d /sys/firmware/efi && echo yes || echo no")
            checks["live image started in the expected firmware mode"] = firmware == ("yes" if variant["uefi"] else "no")

        pointer = Pointer(work / "live" / "vnc.sock", vm.frame)
        inst = Installer(vm, pointer, log=lambda message: print(message, flush=True))
        if not inst.wait_page("Welcome to Bingux", 300):
            raise Fail(f"installer welcome page not reached; on screen: {inst.current_page()!r}")
        checks["live session lands in the installer"] = True
        timeline.mark("page Welcome to Bingux")
        vm.still(out / "pages" / "01-welcome-to-bingux.png", inst.frame())
        inst.continue_page()

        run_steps(inst, vm, variant, out, checks, timeline)

        checks["finished page reports the install"] = text_on_screen(inst.frame(), "Installed on") is not None
        inspect_disk(vm, variant, out, checks, report)

        # Restart as a user does: the medium stays in the drive while the live
        # system shuts down. Only after the guest has reset itself is the medium
        # taken out, and then the firmware is reset so it boots the disk alone.
        reset_mark = vm.event_mark()
        if not inst.click_text("Restart"):
            raise Fail("'Restart' button not found")
        timeline.mark("restart pressed")
        if not vm.wait_reset(reset_mark, 300):
            raise Fail("guest did not reset within 300 s of Restart")
        timeline.mark("guest reset after restart")
        medium = vm.eject_medium()
        checks["installation medium removed after the guest reset"] = medium is not None
        timeline.mark(f"installation medium removed ({medium})")
        vm.system_reset()
        timeline.mark("firmware reset with the disk alone")
        flow_ok = True
    except Fail as error:
        checks[f"installer flow: {error}"] = False
        print(f"FAIL: {error}", flush=True)
        if vm is not None:
            collect_diagnostics(vm, out)
        if vm is not None and vm.frame() is not None:
            vm.still(out / "pages" / "FAILED.png", vm.frame())
    finally:
        if pointer is not None:
            pointer.close()
        if vm is not None and not flow_ok:
            vm.stop()
    if flow_ok:
        try:
            boot_installed(vm, variant, out, timeline, checks)
        finally:
            vm.stop()
    if vm is not None:
        vm.stop()
        (out / "serial.log").write_bytes(vm.log)
    if timeline is not None:
        (out / "timeline.json").write_text(json.dumps(timeline.events, indent=2) + "\n")

    (out / "report.txt").write_text("\n".join(report) + "\n")
    (out / "checks.json").write_text(json.dumps(checks, indent=2) + "\n")
    print(f"--- {name}")
    for label, ok in checks.items():
        print(("ok   " if ok else "FAIL ") + label)
    return bool(checks) and all(checks.values())


def main():
    args = sys.argv[1:]
    name = args.pop(0) if args and args[0] in VARIANTS else "uefi"
    variant = VARIANTS[name]
    iso = find_iso(args[0] if args else None)
    if iso is None or not Path(iso).is_file():
        print("no live image; run distro/build.sh first")
        return 1
    missing = missing_tools("qemu-system-x86_64", "qemu-img", "ffmpeg", "tesseract")
    if missing:
        print("missing tool: " + ", ".join(missing))
        return 1
    out = ROOT / "distro" / "out" / f"install-test-{name}"
    if out.exists():
        shutil.rmtree(out)
    scratch = Path.home() / ".cache" / "bingux-e2e"
    scratch.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="install-", dir=scratch))
    try:
        ok = run_variant(name, variant, iso, out, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print("artifacts:", out)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
