#!/usr/bin/env python3
"""Measure production shell QML frames on private offscreen windows, without desktop services."""

import argparse
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[1]


def replace_once(text, old, new):
    assert text.count(old) == 1, f"Fixture seam changed: {old}"
    return text.replace(old, new)


def prepare(fixture, source, overlays):
    for path in source.iterdir():
        if path.suffix in (".qml", ".js") or path.name == "qmldir":
            shutil.copy2(path, fixture / path.name)
    for path in overlays:
        shutil.copy2(path, fixture / path.name)
    # A VM export may contain only QML/JS; module declarations and bundled
    # static assets come from the pinned checkout, never the shared dirty tree.
    if not (fixture / "qmldir").exists():
        shutil.copy2(REPO / "shell/bingux/qmldir", fixture / "qmldir")
    icons = source / "icons" if (source / "icons").is_dir() else REPO / "shell/bingux/icons"
    shutil.copytree(icons, fixture / "icons")
    # Layer placement/activation need a compositor. Keep each surface's real
    # content, models, bindings and animations, using a FloatingWindow instead.
    for path in fixture.glob("*.qml"):
        text = path.read_text().replace("import Bingux.Wayland\n", "")
        text = re.sub(r"^\s*WlrLayershell\.[^\n]*", "", text, flags=re.M)
        path.write_text(text.replace("PanelWindow {", "IdlePanelWindow {"))
    (fixture / "IdlePanelWindow.qml").write_text("""import QtQuick
import Quickshell
FloatingWindow {
    component Edges: QtObject {
        property real top: 0; property real bottom: 0
        property real left: 0; property real right: 0
    }
    component Anchors: QtObject {
        property bool top: false; property bool bottom: false
        property bool left: false; property bool right: false
    }
    property Edges margins: Edges {}
    property Anchors anchors: Anchors {}
    property int exclusiveZone: 0
    property int exclusionMode: 0
    property bool focusable: false
}""")
    (fixture / "LaunchActivation.qml").write_text("""import QtQuick
QtObject { function request(source, callback) { callback("", "fixture"); } }
""")
    # Controlled platform inputs: no gsettings monitor, audio meter, camera,
    # compositor transport, launch feedback or service activation on the host.
    path = fixture / "Theme.qml"
    path.write_text(path.read_text().replace("running: true", "running: false"))
    (fixture / "Startup.qml").write_text("""pragma Singleton
import QtQuick
QtObject { property bool settled: false; function dockMapped() {} }
""")
    shutil.copy2(REPO / "tests/launch-feedback-mock.qml", fixture / "LaunchFeedback.qml")
    path = fixture / "Dock.qml"
    text = path.read_text().replace("ToplevelManager", "root.testManager")
    text = replace_once(
        text,
        "required property var settings",
        "required property var settings\n    required property var testManager\n    property alias testItems: dockItems",
    )
    text = replace_once(text, "model: root.testManager.toplevels", "model: root.testManager.toplevels.values")
    path.write_text(text)
    # Steam discovery is an external input, unrelated to animation cadence.
    (fixture / "steam-games.py").write_text('print("[]")\n')
    path = fixture / "shell.qml"
    text = path.read_text().replace("import QtQuick.Window", "import QtQuick.Window\nimport QtTest")
    for name in ("topBar", "dock"):
        extra = "\n        testManager: idleManager" if name == "dock" else ""
        text = replace_once(
            text,
            f"        id: {name}\n",
            f"""        id: {name}
        implicitWidth: 1280{extra}
        property int idleFrames: 0
        Connections {{
            target: {name}.contentItem.Window.window
            function onFrameSwapped() {{ {name}.idleFrames++; }}
        }}
""",
        )
    end = text.rfind("}")
    text = text[:end] + (REPO / "tests/shell-idle.inc.qml").read_text() + text[end:]
    path.write_text(text)


def limits():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_CPU, (20, 20))
    os.sched_setaffinity(0, {min(os.sched_getaffinity(0))})


def stop(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=REPO / "shell/bingux")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--renderer", choices=("software", "opengl"), default="software")
    parser.add_argument(
        "--overlay", type=Path, action="append", default=[], help="Replace one component in the private source copy"
    )
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="bingux-idle-") as directory:
        fixture = Path(directory)
        prepare(fixture, args.source.resolve(), args.overlay)
        for name in ("runtime", "config", "state", "cache", "data"):
            (fixture / name).mkdir(mode=0o700)
        (fixture / "config/bingux").mkdir()
        (fixture / "config/bingux/bingux.toml").write_text("")
        # Unlike the normal session bus, this bus has no activation directories.
        # It cannot start portals or any other desktop services.
        bus_config = fixture / "dbus.conf"
        bus_config.write_text(
            "<busconfig><type>session</type>"
            f"<listen>unix:tmpdir={fixture / 'runtime'}</listen>"
            '<auth>EXTERNAL</auth><policy context="default">'
            '<allow send_destination="*"/><allow eavesdrop="true"/><allow own="*"/>'
            "</policy></busconfig>"
        )
        environment = os.environ | {
            "HOME": str(fixture),
            "XDG_RUNTIME_DIR": str(fixture / "runtime"),
            "XDG_CONFIG_HOME": str(fixture / "config"),
            "XDG_STATE_HOME": str(fixture / "state"),
            "XDG_CACHE_HOME": str(fixture / "cache"),
            "XDG_DATA_HOME": str(fixture / "data"),
            "XDG_DATA_DIRS": str(fixture / "data"),
            "DBUS_SYSTEM_BUS_ADDRESS": f"unix:path={fixture}/no-system-bus",
            "PIPEWIRE_RUNTIME_DIR": str(fixture / "runtime"),
            "QT_QPA_PLATFORM": "offscreen",
            "QT_QUICK_BACKEND": "software",
            "BINGUX_REDUCED_MOTION": os.environ.get("BINGUX_REDUCED_MOTION", "0"),
            "BINGUX_LAYOUT_IMPORT": "0",
            "BINGUX_CONFIG_BIN": "/usr/bin/false",
            "BINGUX_ICON_HELPER": "/usr/bin/false",
            "BINGUX_ADWAITA_ICON_DIR": os.environ.get("BINGUX_ADWAITA_ICON_DIR", "/usr/share/icons/Adwaita"),
            "BINGUX_IDLE_REPORT": str(fixture / "report"),
        }
        for key in ("WAYLAND_DISPLAY", "DISPLAY", "SESSION_MANAGER"):
            environment.pop(key, None)
        command = [
            "dbus-run-session",
            f"--config-file={bus_config}",
            "--",
            os.environ.get("QS_TEST_BIN", "qs"),
            "-p",
            str(fixture),
            "--no-color",
        ]
        compositor = None
        if args.renderer == "opengl":
            # Exercise Qt's real scene graph and effects in a private headless
            # Wayland compositor. Mesa and both processes stay on one CPU.
            environment.pop("QT_QUICK_BACKEND")
            environment.update(
                {
                    "QT_QPA_PLATFORM": "wayland",
                    "WAYLAND_DISPLAY": "idle-wayland",
                    "QSG_RHI_BACKEND": "opengl",
                    "LIBGL_ALWAYS_SOFTWARE": "1",
                    "GALLIUM_DRIVER": "llvmpipe",
                    "LP_NUM_THREADS": "1",
                }
            )
            with (fixture / "weston.log").open("w") as log:
                compositor = subprocess.Popen(
                    [
                        "weston",
                        "--backend=headless-backend.so",
                        "--renderer=gl",
                        "--socket=idle-wayland",
                        "--idle-time=0",
                        "--no-config",
                        "--width=1280",
                        "--height=720",
                    ],
                    env=environment,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                    preexec_fn=limits,
                )
            deadline = time.monotonic() + 5
            while not (fixture / "runtime/idle-wayland").exists():
                if compositor.poll() is not None or time.monotonic() > deadline:
                    stop(compositor)
                    raise RuntimeError((fixture / "weston.log").read_text())
                time.sleep(0.05)
        try:
            with (fixture / "runtime.log").open("w") as log:
                process = subprocess.Popen(
                    command,
                    env=environment,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                    preexec_fn=limits,
                )
                try:
                    process.wait(timeout=55)
                finally:
                    stop(process)
        finally:
            if compositor:
                stop(compositor)
        output = (fixture / "runtime.log").read_text()
        report = (fixture / "report").read_text() if (fixture / "report").exists() else "FAIL: no report"
        if args.output:
            args.output.mkdir(parents=True, exist_ok=True)
            (args.output / "runtime.log").write_text(output)
            (args.output / "report").write_text(report)
        print(report)
        if (
            process.returncode
            or "FAILURES 0" not in report
            or any(
                error in output
                for error in ("TypeError:", "ReferenceError:", "Binding loop", "Failed to load configuration")
            )
        ):
            print(output)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
