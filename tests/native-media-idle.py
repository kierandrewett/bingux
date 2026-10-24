#!/usr/bin/env python3
"""Prepare native shell A/B copies; drive controlled real MPRIS and measure existing PIDs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import sys
import subprocess
import threading
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import shell_source  # noqa: E402
BUS_NAME = "org.mpris.MediaPlayer2.bingux_idle_fixture"
OBJECT_PATH = "/org/mpris/MediaPlayer2"
ROOT_IFACE = "org.mpris.MediaPlayer2"
PLAYER_IFACE = ROOT_IFACE + ".Player"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(args):
    if args.output.exists():
        raise SystemExit("Output must be new; existing A/B copies are never overwritten")
    if not (args.assets / "qmldir").is_file():
        raise SystemExit("--assets must be a complete installed shell directory with qmldir/helpers/icons")
    args.output.mkdir(parents=True)
    baseline = args.output / "A"
    candidate = args.output / "B"
    shutil.copytree(args.assets, baseline, symlinks=True)
    exported = {}
    for path in sorted(args.snapshot.iterdir()):
        if path.suffix in (".qml", ".js"):
            (baseline / path.name).unlink(missing_ok=True)
            shutil.copy2(path, baseline / path.name)
            exported[path.name] = digest(path)
    if "shell.qml" not in exported or "MediaControls.qml" not in exported:
        raise SystemExit("Snapshot needs shell.qml and MediaControls.qml")
    shell = baseline / "shell.qml"
    text = shell.read_text()
    assert "native-media-idle" not in text, "Already instrumented"
    text = shell_source.replace_once(text, "import QtQuick.Window", "import QtQuick.Window\nimport QtTest", "test import seam")
    shell.write_text(shell_source.insert_before_root_end(text, (HERE / "native-media-idle.inc.qml").read_text()))
    shutil.copytree(baseline, candidate, symlinks=True)
    (candidate / "MediaControls.qml").unlink()
    shutil.copy2(args.candidate, candidate / "MediaControls.qml")
    differences = [
        str(path.relative_to(baseline))
        for path in sorted(baseline.rglob("*"))
        if path.is_file() and digest(path) != digest(candidate / path.relative_to(baseline))
    ]
    assert differences == ["MediaControls.qml"], differences
    manifest = {
        "snapshot": str(args.snapshot.resolve()),
        "assets": str(args.assets.resolve()),
        "snapshot_sha256": exported,
        "instrumentation_sha256": digest(HERE / "native-media-idle.inc.qml"),
        "instrumented_shell_sha256": digest(shell),
        "A_media_sha256": digest(baseline / "MediaControls.qml"),
        "B_media_sha256": digest(candidate / "MediaControls.qml"),
        "variant_differences": differences,
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(
        json.dumps(
            {
                "A": str(baseline.resolve()),
                "B": str(candidate.resolve()),
                "manifest": str(args.output / "manifest.json"),
            },
            indent=2,
        )
    )


class Player:
    """No periodic signals/audio: Position is monotonic and read on demand."""

    def __init__(self):
        from gi.repository import Gio, GLib

        self.Gio, self.GLib = Gio, GLib
        self.lock = threading.RLock()
        self.calls = []
        self.position_reads = 0
        self.signal_count = 0
        self.base = 42_000_000
        self.started = time.monotonic()
        self.track = 0
        self.playing = False
        self.root = {
            "CanQuit": False,
            "CanRaise": False,
            "HasTrackList": False,
            "Identity": "Bingux idle fixture",
            "DesktopEntry": "bingux-idle-fixture",
            "SupportedUriSchemes": [],
            "SupportedMimeTypes": [],
        }
        self.props = {
            "PlaybackStatus": "Paused",
            "Rate": 1.0,
            "Volume": 1.0,
            "MinimumRate": 1.0,
            "MaximumRate": 1.0,
            "Position": self.base,
            "CanGoNext": True,
            "CanGoPrevious": True,
            "CanPlay": True,
            "CanPause": True,
            "CanSeek": True,
            "CanControl": True,
        }
        self.metadata()

        def properties(values):
            return "".join(
                f'<property name="{key}" type="{self.variant(value).get_type_string()}" access="read"/>'
                for key, value in values.items()
            )

        xml = f'''<node><interface name="{ROOT_IFACE}">{properties(self.root)}
          <method name="Raise"/><method name="Quit"/></interface>
          <interface name="{PLAYER_IFACE}">{properties(self.props)}
          <method name="Play"/><method name="Pause"/><method name="PlayPause"/>
          <method name="Stop"/><method name="Next"/><method name="Previous"/>
          <method name="Seek"><arg type="x" direction="in"/></method>
          <method name="SetPosition"><arg type="o" direction="in"/><arg type="x" direction="in"/></method>
          <signal name="Seeked"><arg type="x"/></signal></interface></node>'''
        self.bus = Gio.DBusConnection.new_for_address_sync(
            os.environ["DBUS_SESSION_BUS_ADDRESS"],
            Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
            None,
            None,
        )
        self.registrations = [
            self.bus.register_object(OBJECT_PATH, interface, self.call, self.get, None)
            for interface in Gio.DBusNodeInfo.new_for_xml(xml).interfaces
        ]
        result = self.bus.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            GLib.Variant("(su)", (BUS_NAME, 4)),
            None,
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        ).unpack()[0]
        if result != 1:
            self.bus.close_sync(None)
            raise RuntimeError("Another idle fixture owns the MPRIS name; it was not replaced")
        self.loop = GLib.MainLoop()
        self.thread = threading.Thread(target=self.loop.run, daemon=True)
        self.thread.start()

    def variant(self, value):
        if isinstance(value, bool):
            signature = "b"
        elif isinstance(value, int):
            signature = "x"
        elif isinstance(value, float):
            signature = "d"
        elif isinstance(value, dict):
            signature = "a{sv}"
        elif isinstance(value, list):
            signature = "as"
        else:
            signature = "s"
        return self.GLib.Variant(signature, value)

    def metadata(self):
        self.props["Metadata"] = {
            "mpris:trackid": self.GLib.Variant("o", f"/track/t{self.track}"),
            "mpris:length": self.GLib.Variant("x", 300_000_000),
            "xesam:title": self.variant("Track A" if self.track % 2 == 0 else "Track B"),
            "xesam:artist": self.variant(["Fixture"]),
        }

    def position(self):
        return min(300_000_000, self.base + int((time.monotonic() - self.started) * 1_000_000) * self.playing)

    def get(self, connection, sender, path, interface, name):
        with self.lock:
            if interface == PLAYER_IFACE and name == "Position":
                self.position_reads += 1
                return self.variant(self.position())
            return self.variant((self.root if interface == ROOT_IFACE else self.props)[name])

    def changed(self, values):
        self.props.update(values)
        self.signal_count += 1
        self.bus.emit_signal(
            None,
            OBJECT_PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            self.GLib.Variant("(sa{sv}as)", (PLAYER_IFACE, {k: self.variant(v) for k, v in values.items()}, [])),
        )

    def seek(self, value):
        self.base = max(0, min(300_000_000, value))
        self.started = time.monotonic()
        self.signal_count += 1
        self.bus.emit_signal(None, OBJECT_PATH, PLAYER_IFACE, "Seeked", self.GLib.Variant("(x)", (self.base,)))

    def playback(self, playing):
        self.base = self.position()
        self.started = time.monotonic()
        self.playing = playing
        self.changed({"PlaybackStatus": "Playing" if playing else "Paused"})

    def reset(self, playing):
        with self.lock:
            self.track = 0
            self.metadata()
            self.changed({"Metadata": self.props["Metadata"]})
            self.playback(playing)
            self.seek(42_000_000)

    def call(self, connection, sender, path, interface, method, parameters, invocation):
        with self.lock:
            args = parameters.unpack()
            self.calls.append({"method": method, "args": args, "monotonic": time.monotonic()})
            if method in ("Play", "Pause", "PlayPause", "Stop"):
                self.playback(method == "Play" or (method == "PlayPause" and not self.playing))
            elif method in ("Next", "Previous"):
                self.track += 1 if method == "Next" else -1
                self.metadata()
                self.changed({"Metadata": self.props["Metadata"]})
                self.seek(0)
            elif method == "Seek":
                self.seek(self.position() + args[0])
            elif method == "SetPosition" and args[0] == f"/track/t{self.track}":
                self.seek(args[1])
            invocation.return_value(None)

    def counters(self):
        with self.lock:
            return {
                "signals": self.signal_count,
                "position_reads": self.position_reads,
                "method_calls": len(self.calls),
            }

    def close(self):
        self.bus.close_sync(None)
        self.loop.quit()
        self.thread.join(timeout=3)


def proc_sample(pid):
    parts = Path(f"/proc/{pid}/stat").read_text().rpartition(")")[2].split()
    return {"ticks": int(parts[11]) + int(parts[12]), "start": int(parts[19])}


def poll(call, predicate, timeout=8):
    deadline = time.monotonic() + timeout
    while True:
        value = call()
        if predicate(value):
            return value
        if time.monotonic() >= deadline:
            raise AssertionError(f"Condition timed out: {value}")
        time.sleep(0.1)


def run(args):
    if not os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
        raise SystemExit("Use the native shell's session bus environment")
    if not os.environ.get("WAYLAND_DISPLAY") and not args.allow_offscreen:
        raise SystemExit(
            "Native run requires WAYLAND_DISPLAY; --allow-offscreen is only for private harness validation"
        )
    if args.output.exists():
        raise SystemExit("Output must be new; evidence is never overwritten")
    args.output.mkdir(parents=True)
    qs = args.quickshell

    def ipc(method, *values):
        result = subprocess.run(
            [
                qs,
                "ipc",
                "--pid",
                str(args.shell_pid),
                "call",
                "native-media-idle",
                method,
                *[str(value) for value in values],
            ],
            capture_output=True,
            text=True,
            timeout=5,
            check=True,
        )
        return json.loads(result.stdout) if result.stdout.strip() else None

    def snapshot():
        return ipc("snapshot")

    first = snapshot()
    assert Path(first["source"].removeprefix("file://")).resolve() == args.source.resolve() / "shell.qml", first
    assert not first["reducedMotion"], "Set BINGUX_REDUCED_MOTION=0 for native A/B"
    identity = {
        name: {"pid": pid, "start": proc_sample(pid)["start"], "executable": os.readlink(f"/proc/{pid}/exe")}
        for name, pid in (("shell", args.shell_pid), ("compositor", args.compositor_pid))
    }
    result = {
        "passed": False,
        "expect": args.expect,
        "driver_sha256": digest(__file__),
        "requested_interval_s": args.seconds,
        "repeats": args.repeats,
        "settle_s": args.settle,
        "source": str(args.source.resolve()),
        "media_sha256": digest(args.source / "MediaControls.qml"),
        "shell_sha256": digest(args.source / "shell.qml"),
        "processes": identity,
        "clock_ticks_per_second": os.sysconf("SC_CLK_TCK"),
        "samples": [],
        "interactions": [],
        "frame_counter": "QQuickWindow.frameSwapped (not a Wayland wire-commit trace)",
    }
    player = Player()
    try:
        poll(lambda: ipc("setup", BUS_NAME), bool)
        poll(snapshot, lambda s: s["busName"] == BUS_NAME and s["mediaWindowIsBar"] and s["mediaVisible"])
        generation = snapshot()["generation"]

        def validate(s, opened, playing):
            assert s["generation"] == generation, "Shell reloaded during measurement"
            assert s["busName"] == BUS_NAME and s["playing"] == playing, s
            assert s["mediaWindowIsBar"] and s["mediaVisible"] and s["barVisible"], s
            assert s["dockVisible"] and s["dockReady"], "Dock must be present in both variants"
            assert s["detailOpen"] == opened and s["seekVisible"] == opened, s
            assert s["titleOverflow"] == 0 and s["marqueeOffset"] == 0, (
                "Long-title scrolling would confound idle frames"
            )

        for phase, playing, opened in (
            ("paused-closed", False, False),
            ("playing-closed", True, False),
            ("playing-open", True, True),
            ("playing-reclosed", True, False),
        ):
            ipc("close")
            player.reset(playing)
            poll(snapshot, lambda s: s["playing"] == playing and s["title"] == "Track A" and not s["detailRetained"])
            time.sleep(args.settle)
            if opened:
                assert ipc("click", "art"), "Artwork click did not reach the real widget"
                poll(snapshot, lambda s: s["seekVisible"] and s["detailOpen"])
            time.sleep(args.settle)
            accepted = 0
            attempts = 0
            while accepted < args.repeats:
                attempts += 1
                assert attempts <= args.repeats + 3, "Too many confounded measurement intervals"
                if phase == "playing-reclosed":
                    # Restart the popup's 30-second native-window cache for
                    # every interval, including retries across a minute edge.
                    assert ipc("click", "art")
                    poll(snapshot, lambda s: s["seekVisible"] and s["detailOpen"])
                    time.sleep(args.settle)
                    ipc("close")
                    poll(snapshot, lambda s: not s["detailRetained"])
                    time.sleep(args.settle)
                before = snapshot()
                validate(before, opened, playing)
                assert before["detailWindowMapped"] == (opened or phase == "playing-reclosed"), (
                    "Use a fresh shell per run; popup cache state must be controlled"
                )
                inputs_before = player.counters()
                cpu_before = {name: proc_sample(data["pid"]) for name, data in identity.items()}
                start = time.monotonic()
                time.sleep(args.seconds)
                elapsed = time.monotonic() - start
                cpu_after = {name: proc_sample(data["pid"]) for name, data in identity.items()}
                after = snapshot()
                inputs_after = player.counters()
                validate(after, opened, playing)
                assert before["detailWindowMapped"] == after["detailWindowMapped"], "Popup cache expired"
                assert before["geometry"] == after["geometry"] and before["dockGroups"] == after["dockGroups"], (
                    "Surface geometry or dock model changed during sample"
                )
                assert inputs_after["signals"] == inputs_before["signals"], "Unexpected fixture input during sample"
                assert inputs_after["method_calls"] == inputs_before["method_calls"], "UI interaction during sample"
                fps_elapsed = (after["wallMs"] - before["wallMs"]) / 1000
                assert fps_elapsed > 0
                cpu = {}
                for name in identity:
                    assert cpu_before[name]["start"] == cpu_after[name]["start"] == identity[name]["start"], (
                        "PID changed"
                    )
                    cpu[name] = (
                        100
                        * (cpu_after[name]["ticks"] - cpu_before[name]["ticks"])
                        / result["clock_ticks_per_second"]
                        / elapsed
                    )
                sample = {
                    "phase": phase,
                    "cpu_interval_s": elapsed,
                    "frame_interval_s": fps_elapsed,
                    "cpu_percent_one_core": cpu,
                    "frames": {key: after["frames"][key] - before["frames"][key] for key in before["frames"]},
                    "ticks": (after["ticks"] - before["ticks"]) % 1_000_000,
                    "position_delta": after["position"] - before["position"],
                    "input_counters_before": inputs_before,
                    "input_counters_after": inputs_after,
                    "before": before,
                    "after": after,
                    "excluded": before["clock"] != after["clock"],
                }
                result["samples"].append(sample)
                (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
                if not sample["excluded"]:
                    accepted += 1
                print(
                    json.dumps(
                        {key: sample[key] for key in ("phase", "frames", "ticks", "cpu_percent_one_core", "excluded")}
                    ),
                    flush=True,
                )

        # Send UI events to real controls, then require actual D-Bus method calls
        # and resulting displayed state. These are Qt test events, not physical HID.
        for label, expected, playing in (("pause", "Pause", False), ("play", "Play", True)):
            count = len(player.calls)
            assert ipc("click", "play")
            poll(snapshot, lambda s: s["playing"] == playing)
            assert any(call["method"] == expected for call in player.calls[count:]), player.calls[count:]
            result["interactions"].append(label)
        count = len(player.calls)
        assert ipc("click", "next")
        poll(snapshot, lambda s: s["title"] == "Track B" and s["displayedTitle"] == "Track B")
        assert any(call["method"] == "Next" for call in player.calls[count:])
        result["interactions"].append("next-track")
        assert ipc("click", "art")
        poll(snapshot, lambda s: s["seekVisible"])
        time.sleep(args.settle)
        count = len(player.calls)
        assert ipc("click", "seek")
        poll(snapshot, lambda s: 160 < s["position"] < 230)
        assert any(call["method"] in ("SetPosition", "Seek") for call in player.calls[count:]), player.calls[count:]
        result["interactions"].append("seek")
        assert ipc("click", "play")
        poll(snapshot, lambda s: not s["playing"])
        ipc("close")
        poll(snapshot, lambda s: not s["detailRetained"])
        with player.lock:
            player.seek(123_000_000)
        time.sleep(args.settle)
        assert ipc("click", "art")
        poll(snapshot, lambda s: s["seekVisible"] and abs(s["position"] - 123) < 0.1)
        result["interactions"].append("paused-reopen-current-position")
        ipc("close")
        result["method_calls"] = player.calls
        result["summary"] = summary(result)
        closed = [
            s for s in result["samples"] if not s["excluded"] and s["phase"] in ("playing-closed", "playing-reclosed")
        ]
        opened = [s for s in result["samples"] if not s["excluded"] and s["phase"] == "playing-open"]
        assert all(s["ticks"] > 0 and s["position_delta"] > args.seconds * 0.5 for s in opened), (
            "Visible playback stopped"
        )
        if args.expect == "fixed":
            assert all(s["ticks"] == 0 for s in closed), "Hidden playhead still ticks"
        else:
            assert all(s["ticks"] / s["frame_interval_s"] > 10 for s in closed), "Baseline bug did not reproduce"
        result["passed"] = True
    except Exception as error:
        result["passed"] = False
        result["error"] = str(error)
        raise
    finally:
        try:
            ipc("restore")
        finally:
            player.close()
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["summary"], indent=2))


def summary(result):
    rows = {}
    for phase in ("paused-closed", "playing-closed", "playing-open", "playing-reclosed"):
        samples = [s for s in result["samples"] if s["phase"] == phase and not s["excluded"]]
        if samples:
            rows[phase] = {
                "intervals": len(samples),
                "median_cpu_percent_one_core": {
                    key: statistics.median(s["cpu_percent_one_core"][key] for s in samples)
                    for key in ("shell", "compositor")
                },
                "median_fps": {
                    key: statistics.median(s["frames"][key] / s["frame_interval_s"] for s in samples)
                    for key in ("bar", "dock", "details")
                },
                "median_playhead_ticks_per_s": statistics.median(s["ticks"] / s["frame_interval_s"] for s in samples),
            }
    return rows


def compare(args):
    before, after = [json.loads(path.read_text()) for path in (args.baseline, args.candidate)]
    assert before.get("passed") and after.get("passed"), "Both runs must complete their interaction checks"
    assert before["expect"] == "baseline" and after["expect"] == "fixed", "Pass baseline then candidate"
    assert before["requested_interval_s"] == after["requested_interval_s"] and before["repeats"] == after["repeats"], (
        "Use equal intervals/repeats"
    )
    assert before["shell_sha256"] == after["shell_sha256"], "Instrumentation/shell changed between variants"
    assert before["driver_sha256"] == after["driver_sha256"] and before["settle_s"] == after["settle_s"], (
        "Driver or settle interval changed between variants"
    )
    assert before["processes"]["compositor"] == after["processes"]["compositor"], "Compositor changed between variants"
    for phase in ("paused-closed", "playing-closed", "playing-open", "playing-reclosed"):
        left, right = [next(s["before"] for s in run["samples"] if s["phase"] == phase) for run in (before, after)]
        assert left["geometry"] == right["geometry"] and left["dockGroups"] == right["dockGroups"], (
            "Surface geometry or dock model differs between variants"
        )
    report = {"baseline": summary(before), "candidate": summary(after), "cpu_units": "percent of one core"}
    print(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare", help="Create A/B shell copies; never launches a shell or services")
    p.add_argument("--snapshot", type=Path, required=True)
    p.add_argument("--assets", type=Path, required=True)
    p.add_argument("--candidate", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.set_defaults(function=prepare)
    p = sub.add_parser("run", help="Measure the explicitly selected, already-running native shell")
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--shell-pid", type=int, required=True)
    p.add_argument("--compositor-pid", type=int, required=True)
    p.add_argument("--quickshell", default=os.environ.get("QS_TEST_BIN", "qs"))
    p.add_argument("--expect", choices=("baseline", "fixed"), required=True)
    p.add_argument("--seconds", type=float, default=8)
    p.add_argument("--settle", type=float, default=2)
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--allow-offscreen", action="store_true", help=argparse.SUPPRESS)
    p.set_defaults(function=run)
    p = sub.add_parser("compare")
    p.add_argument("baseline", type=Path)
    p.add_argument("candidate", type=Path)
    p.set_defaults(function=compare)
    args = parser.parse_args()
    if args.command == "run" and not (0.5 <= args.seconds <= 20 and 0.5 <= args.settle <= 5 and 1 <= args.repeats <= 5):
        parser.error("Require 0.5..20 second intervals, 0.5..5 second settle, and 1..5 repeats")
    args.function(args)


if __name__ == "__main__":
    main()
