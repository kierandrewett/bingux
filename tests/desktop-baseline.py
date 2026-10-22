#!/usr/bin/env python3
"""Screenshot the production desktop top bar and full output, then compare with references.

Runs the real shell/bingux tree under headless Sway at 1280x800 and captures
the full output and the top bar strip (y 0..Theme.barHeight). Frames are taken
until two consecutive captures are byte-identical, so an animating shell is not
compared mid-transition. The run fails if the shell reports a load error or the
top bar is not drawn, so an empty desktop can never become the reference.

Usage:
  tests/desktop-baseline.py            compare with tests/fixtures/desktop-baseline/
  tests/desktop-baseline.py --update   rewrite the references (review the diff)

Tolerance: a pixel differs when any channel moves by more than CHANNEL_TOLERANCE
(of 255). The check fails when more than FRACTION_TOLERANCE of a compared image's
pixels differ. MASKED regions hold live content (the clock) and are blacked out
in both images before comparison.

Requires headless sway, grim, qs (QS_TEST_BIN overrides) and PIL.

Hermetic: the run always re-executes itself under dbus-run-session, with the
system bus pointed at a missing socket, so no host tray items, MPRIS or BlueZ
services leak into the bar. Metrics come from the greeter-ui FakeMetrics stub,
so CPU/RAM read fixed values. Regenerate references only in this environment.
"""

import argparse
import importlib.util
import os
import sys
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "shell/bingux"
REFERENCE = ROOT / "tests/fixtures/desktop-baseline"
HERMETIC_FLAG = "BINGUX_BASELINE_HERMETIC"
QS = os.environ.get("QS_TEST_BIN", "qs")
OUTPUT = "HEADLESS-1"
SYSTEM_BUS = "unix:path=/nonexistent/bingux-hermetic-system-bus"
SIZE = (1280, 800)
CHANNEL_TOLERANCE = 8
FRACTION_TOLERANCE = 0.002
SETTLE_TIMEOUT = 45.0


def ensure_hermetic():
    """Re-exec under a private session bus with no reachable system bus."""
    if os.environ.get(HERMETIC_FLAG) == "1":
        return
    environment = os.environ | {HERMETIC_FLAG: "1"}
    os.execvpe("dbus-run-session", ["dbus-run-session", "--", sys.executable, *sys.argv], environment)


def fake_metrics(runtime):
    """Start the greeter-ui metrics-v1 stub so the bar shows fixed CPU/RAM values."""
    spec = importlib.util.spec_from_file_location("greeter_ui", ROOT / "tests/greeter-ui.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    stub = module.FakeMetrics(runtime / "bingux/metrics-v1.sock")
    stub.start()
    return stub


# (left, top, right, bottom) in output pixels. The clock (x 575-703) and the CPU/RAM
# readout (x 986-1030) change while the shell runs; their boxes are blacked out.
MASKED = [(570, 0, 710, 32), (980, 0, 1040, 32)]
MIN_BAR_COLOURS = 8


def wait_for(predicate, timeout=15.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.1)
    return False


def bar_height():
    match = re.search(r"readonly property int barHeight: (\d+)", (SHELL / "Theme.qml").read_text())
    if not match:
        raise SystemExit("desktop-baseline: Theme.barHeight not found in shell/bingux/Theme.qml")
    return int(match.group(1))


def mask(image):
    image = image.copy()
    for box in MASKED:
        image.paste((0, 0, 0), box)
    return image


def capture(environment, frame):
    subprocess.run(["grim", "-o", OUTPUT, str(frame)], env=environment, check=True, capture_output=True)
    return Image.open(frame).convert("RGB")


def drawn_top_bar(image):
    top = image.crop((0, 0, SIZE[0], bar_height()))
    return len(set(top.getdata())) >= MIN_BAR_COLOURS


def settled_desktop(environment, work, log):
    """Return the first full-output frame that is identical to the one before it and shows the top bar."""
    previous = None
    deadline = time.monotonic() + SETTLE_TIMEOUT
    frame = work / "frame.png"
    while time.monotonic() < deadline:
        if "Failed to load configuration" in log.read_text(errors="replace"):
            raise SystemExit("desktop-baseline: shell failed to load\n" + log.read_text(errors="replace"))
        current = capture(environment, frame)
        if previous is not None and current.tobytes() == previous.tobytes() and drawn_top_bar(current):
            return current
        previous = current
        time.sleep(0.5)
    raise SystemExit(
        f"desktop-baseline: no settled desktop with a drawn top bar within {SETTLE_TIMEOUT:.0f}s\n"
        + log.read_text(errors="replace")
    )


def capture_desktop():
    """Run headless sway with the production shell; return (full, top bar) images."""
    home = Path(tempfile.mkdtemp(prefix="bingux-desktop-baseline-"))
    work = home / "work"
    work.mkdir()
    runtime = home / "run"
    runtime.mkdir(mode=0o700)
    metrics = fake_metrics(runtime)
    sway_config = home / "sway.conf"
    sway_config.write_text(f"output {OUTPUT} resolution {SIZE[0]}x{SIZE[1]}\n")
    base = os.environ | {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "WLR_BACKENDS": "headless",
        "WLR_RENDERER": "gles2",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "BINGUX_SOFTWARE_RENDERING": os.environ.get("BINGUX_SOFTWARE_RENDERING", "1"),
        "DBUS_SYSTEM_BUS_ADDRESS": SYSTEM_BUS,
    }
    base.pop("DISPLAY", None)
    base.pop("WAYLAND_DISPLAY", None)
    sway = subprocess.Popen(["sway", "-c", str(sway_config)], env=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    client = None
    try:
        if not wait_for(lambda: any(not p.name.endswith(".lock") for p in runtime.glob("wayland-*"))):
            raise SystemExit("desktop-baseline: headless sway did not start")
        wayland = sorted(p.name for p in runtime.glob("wayland-*") if not p.name.endswith(".lock"))[0]
        ipc = sorted(runtime.glob("sway-ipc.*.sock"))
        if not ipc:
            raise SystemExit("desktop-baseline: headless sway IPC socket did not appear")
        environment = base | {"WAYLAND_DISPLAY": wayland, "SWAYSOCK": str(ipc[0]), "QT_QPA_PLATFORM": "wayland"}
        log = work / "shell.log"
        handle = log.open("w")
        client = subprocess.Popen(
            [QS, "-p", str(SHELL), "--no-color"], env=environment, stdout=handle, stderr=subprocess.STDOUT
        )
        full = settled_desktop(environment, work, log)
        return full, full.crop((0, 0, SIZE[0], bar_height()))
    finally:
        if client is not None and client.poll() is None:
            client.kill()
            client.wait(timeout=5)
        sway.terminate()
        sway.wait(timeout=5)
        metrics.stop.set()
        shutil.rmtree(home, ignore_errors=True)


def changed_fraction(reference, actual):
    """Fraction of pixels where some channel differs by more than CHANNEL_TOLERANCE."""
    if reference.size != actual.size:
        raise SystemExit(f"desktop-baseline: size changed {reference.size} -> {actual.size}")
    diff = ImageChops.difference(mask(reference), mask(actual))
    red, green, blue = diff.split()
    widest = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    over = widest.point(lambda value: 255 if value > CHANNEL_TOLERANCE else 0)
    count = over.histogram()[255]
    return count / (reference.size[0] * reference.size[1]), count


def main():
    ensure_hermetic()
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--update", action="store_true", help="rewrite the reference PNGs")
    arguments = parser.parse_args()
    full, top = capture_desktop()
    targets = {"desktop.png": full, "top-bar.png": top}
    if arguments.update:
        REFERENCE.mkdir(parents=True, exist_ok=True)
        for name, image in targets.items():
            image.save(REFERENCE / name, optimize=True)
            print(f"wrote {REFERENCE.relative_to(ROOT) / name} {image.size}")
        return
    failed = False
    for name, image in targets.items():
        reference_path = REFERENCE / name
        if not reference_path.is_file():
            raise SystemExit(f"desktop-baseline: missing reference {reference_path}; run with --update")
        fraction, count = changed_fraction(Image.open(reference_path).convert("RGB"), image)
        status = "ok  " if fraction <= FRACTION_TOLERANCE else "FAIL"
        print(f"{status} {name}: {count} px changed ({fraction:.5%}, tolerance {FRACTION_TOLERANCE:.3%})")
        failed = failed or fraction > FRACTION_TOLERANCE
    if failed:
        raise SystemExit("desktop-baseline: drift above tolerance")


if __name__ == "__main__":
    main()
