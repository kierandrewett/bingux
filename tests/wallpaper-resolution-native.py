#!/usr/bin/env python3
"""Native guest regression for Wallpaper.qml pixel-resolution decoding.

Run only inside an explicitly marked disposable QEMU/KVM guest. The runner
temporarily replaces the guest's one managed WallpaperShell.qml process and
restarts guest GDM in all exit paths.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from PIL import ImageChops
from PIL import Image, ImageDraw


REPO = Path(__file__).resolve().parents[1]
SETTLE_SECONDS = 3.0
FADE_SECONDS = 1.2
RGB_TOLERANCE = 2


def run(command, *, timeout=10, check=True, capture=True):
    return subprocess.run(
        command,
        check=check,
        timeout=timeout,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )


def require_guest():
    if os.environ.get("BINGUX_DISPOSABLE_QEMU_GUEST") != "1":
        raise RuntimeError("set BINGUX_DISPOSABLE_QEMU_GUEST=1 only in the disposable test guest")
    virtual = run(["systemd-detect-virt", "--vm"]).stdout.strip()
    if virtual not in {"qemu", "kvm"}:
        raise RuntimeError(f"expected a QEMU/KVM guest, detected {virtual!r}")
    product = Path("/sys/class/dmi/id/product_name").read_text(errors="replace").strip()
    if "qemu" not in product.lower() and "standard pc" not in product.lower():
        raise RuntimeError(f"guest DMI product does not identify QEMU: {product!r}")
    if not os.environ.get("WAYLAND_DISPLAY") or not os.environ.get("XDG_RUNTIME_DIR"):
        raise RuntimeError("the active guest Wayland session environment must be inherited")
    if not shutil.which("grim"):
        raise RuntimeError("grim is required in the guest")
    run(["sudo", "-n", "-l", "systemctl", "restart", "gdm"], timeout=5)


def monitor_info():
    data = json.loads(run(["gnoblinctl", "--json", "monitor", "list"]).stdout)
    monitors = data["monitors"]
    if len(monitors) != 1:
        raise RuntimeError(f"expected exactly one guest monitor; got {len(monitors)}")
    return monitors[0]


def process_identity(pid):
    proc = Path(f"/proc/{pid}")
    raw = (proc / "cmdline").read_bytes().split(b"\0")
    argv = [item.decode(errors="replace") for item in raw if item]
    exe = os.readlink(proc / "exe")
    stat = (proc / "stat").read_text()
    # starttime is field 22; comm can contain spaces and parentheses.
    fields = stat[stat.rfind(")") + 2 :].split()
    return exe, argv, fields[19]


def managed_wallpaper(wrapper):
    matches = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdecimal():
            continue
        try:
            exe, argv, start = process_identity(entry.name)
        except (FileNotFoundError, ProcessLookupError, PermissionError, IndexError):
            continue
        joined = "\0".join(argv)
        if "WallpaperShell.qml" in joined and argv and Path(argv[0]).resolve() == wrapper.resolve():
            matches.append((int(entry.name), exe, argv, start))
    if len(matches) != 1:
        raise RuntimeError(f"expected one managed Quickshell WallpaperShell.qml process; found {len(matches)}")
    pid, exe, argv, start = matches[0]
    if not any(Path(arg).name == "WallpaperShell.qml" for arg in argv):
        raise RuntimeError(f"PID {pid} does not name WallpaperShell.qml as an argv entry")
    return pid, exe, argv, start


def capture(path):
    run(["grim", str(path)], timeout=15)
    with Image.open(path) as image:
        image.load()
        if image.width < 320 or image.height < 320:
            raise RuntimeError(f"capture dimensions are implausible: {image.size}")
        return image.convert("RGB")


def fixture_image(path, width, height, variant):
    image = Image.new("RGB", (width, height), (17, 29, 43))
    draw = ImageDraw.Draw(image)
    colors = ((241, 31, 47), (31, 230, 79), (43, 79, 242))
    if variant:
        colors = ((241, 206, 31), (31, 211, 230), (223, 31, 210))
    for index, color in enumerate(colors):
        left = index * width // 3
        right = (index + 1) * width // 3
        draw.rectangle((left, 0, right - 1, height - 1), fill=color)
    # Thin alternating pixel lines make scaling and interpolation visible.
    for x in range(0, width, 19):
        draw.line((x, 0, x, height - 1), fill=(250, 250, 250), width=2)
    for y in range(0, height, 23):
        draw.line((0, y, width - 1, y), fill=(8, 8, 8), width=2)
    draw.rectangle((width // 3, height // 3, 2 * width // 3, 2 * height // 3), outline=(255, 255, 255), width=7)
    image.save(path, format="PNG", optimize=False)
    image.close()


def qml_fixture(shell_dir, wallpaper_source, first_url):
    (shell_dir / "qmldir").write_text("singleton BinguxPreferences 1.0 BinguxPreferences.qml\n")
    (shell_dir / "BinguxPreferences.qml").write_text(
        f'pragma Singleton\nimport QtQuick\nQtObject {{ property url wallpaperUrl: "{first_url}" }}\n'
    )
    shutil.copyfile(wallpaper_source, shell_dir / "Wallpaper.qml")
    (shell_dir / "shell.qml").write_text(
        """//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    Wallpaper {}
    IpcHandler {
        target: "wallpaper-native-resolution"
        function setWallpaper(value: string): void {
            BinguxPreferences.wallpaperUrl = value
        }
    }
}
"""
    )


def check_pixels(source_path, shot_path, scale):
    with Image.open(source_path) as expected_raw, Image.open(shot_path) as actual_raw:
        expected = expected_raw.convert("RGB")
        actual = actual_raw.convert("RGB")
        # Keep away from the top bar, right sidebar, bottom dock, and the
        # recovery notice raised when the managed wallpaper receives SIGTERM.
        x0, y0 = 0, max(240, math.ceil(160 * scale))
        x1 = actual.width - math.ceil(32 * scale)
        y1 = actual.height - max(160, math.ceil(80 * scale))
        if x1 <= x0 or y1 <= y0:
            raise RuntimeError(f"capture is too small for the stable comparison ROI: {actual.size}")
        if expected.size != actual.size:
            raise RuntimeError(f"fixture/capture dimensions differ: {expected.size} != {actual.size}")
        roi_a = expected.crop((x0, y0, x1, y1))
        roi_b = actual.crop((x0, y0, x1, y1))
        diff = ImageChops.difference(roi_a, roi_b)
        max_error = max(channel.getextrema()[1] for channel in diff.split())
        if roi_b.getbbox() is None:
            raise RuntimeError("captured comparison region is blank")
        return max_error, roi_a.size


def wait_child(process, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"test Quickshell exited early with status {process.returncode}")
        time.sleep(0.1)


def terminate_owned(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected-scale", type=float, help="fail preflight unless the monitor has this scale")
    parser.add_argument("--source", type=Path, default=REPO / "shell/bingux/Wallpaper.qml")
    parser.add_argument(
        "--quickshell",
        type=Path,
        required=True,
        help="absolute guest Quickshell executable used by the managed wallpaper",
    )
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        required=True,
        help="directory for retained captures and JSON result",
    )
    args = parser.parse_args()
    if not args.quickshell.is_absolute() or not args.quickshell.is_file():
        parser.error("--quickshell must name an existing absolute executable")
    source_path = args.source.resolve()
    if not source_path.is_file():
        parser.error(f"Wallpaper.qml source does not exist: {source_path}")

    require_guest()
    monitor = monitor_info()
    if args.expected_scale is not None and not math.isclose(
        monitor.get("scale", 0), args.expected_scale, rel_tol=0, abs_tol=0.001
    ):
        raise RuntimeError(f"expected monitor scale {args.expected_scale}; got {monitor.get('scale')}")
    source_hash = hashlib.sha256(source_path.read_bytes()).hexdigest()
    managed = managed_wallpaper(args.quickshell)
    capture_fd, initial_capture_name = tempfile.mkstemp(prefix="bingux-wallpaper-before-", suffix=".png")
    os.close(capture_fd)
    initial_capture_path = Path(initial_capture_name)
    try:
        initial = capture(initial_capture_path)
    except BaseException:
        initial_capture_path.unlink(missing_ok=True)
        raise
    width, height = initial.size
    initial.close()
    metadata_scale = monitor.get("scale", monitor.get("scaleFactor"))
    logical_width, logical_height = monitor.get("width"), monitor.get("height")
    if not isinstance(metadata_scale, (int, float)) or metadata_scale <= 0:
        initial_capture_path.unlink(missing_ok=True)
        raise RuntimeError(f"invalid monitor scale metadata: {metadata_scale!r}")
    try:
        derived_pixels = [round(logical_width * metadata_scale), round(logical_height * metadata_scale)]
    except TypeError as error:
        initial_capture_path.unlink(missing_ok=True)
        raise RuntimeError(f"invalid logical monitor dimensions: {(logical_width, logical_height)!r}") from error
    if derived_pixels != [width, height]:
        initial_capture_path.unlink(missing_ok=True)
        raise RuntimeError(
            f"monitor logical size and scale imply {derived_pixels}, while grim captured {[width, height]}"
        )

    artifact_dir = args.artifact_dir.resolve()
    if artifact_dir.exists() and any(artifact_dir.iterdir()):
        initial_capture_path.unlink(missing_ok=True)
        raise RuntimeError(f"artifact directory must be empty: {artifact_dir}")
    artifact_dir.mkdir(parents=True, exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix="bingux-wallpaper-native-"))
    child = None
    managed_pidfd = None
    stopped_identity = None
    session_displaced = False
    failure = None
    restoration_error = None
    guest_gdm_restored = False
    error_a = None
    error_b = None
    try:
        if workspace == artifact_dir or workspace in artifact_dir.parents:
            raise RuntimeError("--artifact-dir must be outside the private staging directory")
        image_a = workspace / "native-a.png"
        image_b = workspace / "native-b.png"
        fixture_image(image_a, width, height, 0)
        fixture_image(image_b, width, height, 1)
        qml_fixture(workspace, source_path, image_a.as_uri())

        pid, exe, argv, start = managed
        if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
            raise RuntimeError("this guest Python/kernel must support pidfd signaling for exact process cleanup")
        managed_pidfd = os.pidfd_open(pid)
        current = process_identity(pid)
        if current != (exe, argv, start):
            raise RuntimeError("managed wallpaper process identity changed during preflight")
        session_displaced = True
        signal.pidfd_send_signal(managed_pidfd, signal.SIGTERM)
        os.close(managed_pidfd)
        managed_pidfd = None
        stopped_identity = {"pid": pid, "exe": exe, "argv": argv, "starttime": start}
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            try:
                if process_identity(pid)[2] != start:
                    break
            except (FileNotFoundError, ProcessLookupError):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(f"managed wallpaper PID {pid} did not stop after SIGTERM")

        log_path = workspace / "quickshell.log"
        log = log_path.open("w")
        env = os.environ.copy()
        child = subprocess.Popen(
            [str(args.quickshell), "-p", str(workspace), "--no-color"],
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        wait_child(child, SETTLE_SECONDS + FADE_SECONDS)
        shot_a = workspace / "capture-a.png"
        image = capture(shot_a)
        image.close()
        error_a, roi_size = check_pixels(image_a, shot_a, metadata_scale)
        if error_a > RGB_TOLERANCE:
            raise RuntimeError(f"native source A max RGB error {error_a} exceeds {RGB_TOLERANCE}")

        run(
            [
                str(args.quickshell),
                "ipc",
                "-p",
                str(workspace),
                "call",
                "wallpaper-native-resolution",
                "setWallpaper",
                image_b.as_uri(),
            ],
            timeout=5,
        )
        time.sleep(SETTLE_SECONDS + FADE_SECONDS)
        shot_b = workspace / "capture-b.png"
        image = capture(shot_b)
        image.close()
        error_b, _ = check_pixels(image_b, shot_b, metadata_scale)
        if error_b > RGB_TOLERANCE:
            raise RuntimeError(f"native source B max RGB error {error_b} exceeds {RGB_TOLERANCE}")
        log.close()
        shutil.copy2(shot_a, artifact_dir / "capture-a.png")
        shutil.copy2(shot_b, artifact_dir / "capture-b.png")
        shutil.copy2(image_a, artifact_dir / "source-a.png")
        shutil.copy2(image_b, artifact_dir / "source-b.png")
        shutil.copy2(log_path, artifact_dir / "quickshell.log")
        result = {
            "result": "PASS",
            "source": str(source_path),
            "source_sha256": source_hash,
            "managed_child": stopped_identity,
            "test_child": {
                "pid": child.pid,
                "identity": process_identity(child.pid) if child.poll() is None else None,
            },
            "monitor_logical_pixels": [logical_width, logical_height],
            "monitor_scale": metadata_scale,
            "logical_times_scale_pixels": derived_pixels,
            "capture_pixels": [width, height],
            "monitor_metadata": monitor,
            "roi_pixels": list(roi_size),
            "max_rgb_error": {"first": error_a, "second": error_b},
            "tolerance": RGB_TOLERANCE,
            "artifacts": str(artifact_dir),
        }
        (artifact_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    except BaseException as exc:
        failure = exc
        failure_record = {
            "result": "FAIL",
            "error": f"{type(exc).__name__}: {exc}",
            "source": str(source_path),
            "source_sha256": source_hash,
            "managed_child": stopped_identity,
            "monitor_metadata": monitor,
            "capture_pixels": [width, height],
            "max_rgb_error": {"first": error_a, "second": error_b},
            "tolerance": RGB_TOLERANCE,
            "artifacts": str(artifact_dir),
        }
        if "log_path" in locals() and log_path.exists():
            shutil.copy2(log_path, artifact_dir / "quickshell.log")
        for name in ("capture-a.png", "capture-b.png"):
            staged = workspace / name
            if staged.exists():
                shutil.copy2(staged, artifact_dir / name)
        for name in ("native-a.png", "native-b.png"):
            staged = workspace / name
            if staged.exists():
                shutil.copy2(staged, artifact_dir / name.replace("native-", "source-"))
        (artifact_dir / "result.json").write_text(json.dumps(failure_record, indent=2) + "\n")
    finally:
        if managed_pidfd is not None:
            os.close(managed_pidfd)
        if child is not None:
            try:
                terminate_owned(child)
            except BaseException as child_error:
                if failure is None:
                    failure = RuntimeError(f"could not stop owned test Quickshell process: {child_error}")
        if "log" in locals() and not log.closed:
            log.close()
        if session_displaced:
            try:
                # This is reached only after require_guest() has positively
                # identified the explicitly opted-in QEMU/KVM guest.
                run(["sudo", "-n", "systemctl", "restart", "gdm"], timeout=30, capture=False)
                guest_gdm_restored = True
            except BaseException as restore_error:
                restoration_error = str(restore_error)
                if failure is None:
                    failure = RuntimeError(f"guest GDM restoration failed: {restore_error}")
                else:
                    print(f"ERROR: guest GDM restoration failed: {restore_error}", file=sys.stderr)
        initial_capture_path.unlink(missing_ok=True)
        shutil.rmtree(workspace, ignore_errors=True)
    current = json.loads((artifact_dir / "result.json").read_text())
    current["guest_gdm_restart_succeeded"] = guest_gdm_restored
    if restoration_error is not None:
        current["guest_gdm_restore_error"] = restoration_error
    if failure is not None:
        current["result"] = "FAIL"
        current["error"] = f"{type(failure).__name__}: {failure}"
    (artifact_dir / "result.json").write_text(json.dumps(current, indent=2) + "\n")
    if failure is not None:
        print(json.dumps(current, indent=2), file=sys.stderr)
        raise failure
    print(json.dumps(current, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, subprocess.SubprocessError, RuntimeError, ValueError, AssertionError) as error:
        raise SystemExit(f"FAIL: {error}")
