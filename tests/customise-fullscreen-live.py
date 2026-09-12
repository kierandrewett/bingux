#!/usr/bin/env python3
"""Check editor stacking and real input above a fullscreen app in a private session."""

import ast
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import tempfile
import time
from private_shell import stage_compositor_bridge

repo = Path(__file__).resolve().parents[1]
config = Path(os.environ["XDG_CONFIG_HOME"])
assert str(config).startswith("/tmp/gnoblin-gs.")
assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
qs = os.environ.get("QS_TEST_BIN", "qs")


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=8, **kwargs).stdout.strip()


scripts = stage_compositor_bridge(repo, config)
(scripts / "customise-stacking-test.js").write_text("""
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
export default function (api) {
    const service = Gio.DBusExportedObject.wrapJSObject(
        '<node><interface name="org.gnoblin.CustomiseStack"><method name="State"><arg type="s" direction="out"/></method></interface></node>', {
        State() {
            const order = [];
            function visit(actor) {
                if (!actor.visible) return;
                if (actor.meta_window) order.push(Meta.gnoblin_layer_namespace(actor.meta_window) || actor.meta_window.title);
                for (const child of actor.get_children()) visit(child);
            }
            visit(global.stage);
            return JSON.stringify(order);
        }
    });
    service.export(Gio.DBus.session, '/org/gnoblin/CustomiseStack');
    const name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnoblin.CustomiseStack', Gio.BusNameOwnerFlags.NONE, null, null, null);
    api._disposers.push(() => { service.unexport(); Gio.bus_unown_name(name); });
}
""")
with tempfile.TemporaryDirectory(prefix="bingux-editor-fullscreen-") as directory:
    fixture = Path(directory)
    qml = fixture / "qml"
    shutil.copytree(repo / "shell/bingux", qml)
    customiser = qml / "DesktopCustomise.qml"
    customiser.write_text(
        customiser.read_text().replace(
            "id: root", "id: root\n    readonly property var testStacking: stackingSession", 1
        )
    )
    shell = qml / "shell.qml"
    source = shell.read_text().replace("import QtQuick\n", "import QtQuick\nimport QtQuick.Window\n", 1)
    shell.write_text(
        source.rstrip()[:-1]
        + """
    Window { id: fullscreenFixture; visible: true; width: 640; height: 400; title: "Customise fullscreen fixture"; color: "#e50045" }
    IpcHandler {
        target: "editor-stack-test"
        function fullscreen(): void { fullscreenFixture.showFullScreen(); }
        function open(): void { binguxSettings.read(); }
        function edit(): void { terminalSidebar.selectContent("notes"); desktopCustomiser.open(); }
        function cancel(): void { desktopCustomiser.cancel(); }
        function status(): string {
            const palette = desktopCustomiser.preview.paletteRect;
            const stacking = desktopCustomiser.testStacking;
            function findInput(item) {
                if (item.objectName === "customiseFilterInput") return item;
                for (const child of item.children || []) { const found = findInput(child); if (found) return found; }
                return null;
            }
            const input = findInput(desktopCustomiser.contentItem);
            const point = input ? DesktopEditing.point(input, desktopCustomiser.nativeWindow, 20, input.height / 2) : null;
            return JSON.stringify({loaded: BinguxPreferences.loaded, ready: binguxSettings.ready, busy: binguxSettings.busy,
                visible: desktopCustomiser.visible, fullscreen: fullscreenFixture.visibility === Window.FullScreen,
                input: point, stacking: stacking ? {capabilities: stacking.capabilities, state: stacking.state, connected: stacking.connected} : null, filter: desktopCustomiser.appFilter, palette: {x: palette.x, y: palette.y, width: palette.width, height: palette.height}});
        }
    }
}
"""
    )
    environment = os.environ | {
        "BINGUX_TEST_COMPOSITOR_CONFIG": str(config),
        "XDG_CONFIG_HOME": str(fixture / "config"),
        "XDG_STATE_HOME": str(fixture / "state"),
        "BINGUX_LAYOUT_IMPORT": "0",
    }
    environment.pop("BINGUX_SETTINGS_HELPER", None)
    native = repo / "tests/customise-native-input.py"
    run(["python3", str(native), "--prepare"], env=environment)
    log = (fixture / "runtime.log").open("w+")
    process = subprocess.Popen(
        [qs, "-p", str(qml), "--no-color"],
        env=environment,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    def call(method):
        value = run([qs, "-p", str(qml), "ipc", "call", "editor-stack-test", method], env=environment)
        return json.loads(value) if value else None

    def order():
        value = run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.gnoblin.CustomiseStack",
                "--object-path",
                "/org/gnoblin/CustomiseStack",
                "--method",
                "org.gnoblin.CustomiseStack.State",
            ]
        )
        return json.loads(ast.literal_eval(value)[0])

    def wait(predicate, label):
        deadline = time.monotonic() + 8
        last = None
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise AssertionError(("Shell exited", process.returncode))
            try:
                last = predicate()
                if last:
                    return last
            except (ValueError, subprocess.SubprocessError):
                pass
            time.sleep(0.08)
        raise AssertionError((label, last, call("status"), order()))

    try:
        wait(lambda: call("status")["loaded"], "preferences loaded")
        call("open")
        wait(lambda: call("status")["ready"] and not call("status")["busy"], "settings loaded")
        wait(lambda: "Customise fullscreen fixture" in order(), "fixture mapped before entering fullscreen")
        call("fullscreen")
        wait(lambda: call("status")["fullscreen"], "fixture fullscreen")
        time.sleep(0.3)
        call("edit")
        wait(lambda: call("status")["visible"], "editor open")
        containers = ["bingux-top-bar", "bingux-dock", "bingux-terminal-sidebar", "gnoblin-shell-popup"]

        def correctly_stacked():
            stack = order()
            return all(
                name in stack for name in ["Customise fullscreen fixture", "bingux-customise", *containers]
            ) and all(
                stack.index("Customise fullscreen fixture") < stack.index("bingux-customise") < stack.index(name)
                for name in containers
            )

        wait(correctly_stacked, "actual containers above editor above fullscreen app")
        state = call("status")
        run(["python3", str(native), str(state["input"]["x"]), str(state["input"]["y"])], env=environment)
        wait(lambda: call("status")["filter"] == "Find", "native click and typing reach the palette")
        wait(correctly_stacked, "clicking the palette preserves container order")
        capture = os.environ.get("BINGUX_EDITOR_FULLSCREEN_CAPTURE")
        if capture:
            run(["grim", capture])
        call("cancel")
        wait(lambda: not call("status")["visible"], "editor cancelled")
        assert call("status")["fullscreen"], "Editing must not change the app fullscreen state"
        log.flush()
        log.seek(0)
        output = log.read()
        assert not any(
            error in output
            for error in ("TypeError", "ReferenceError", "has crashed", "Cannot use same item on different windows")
        ), output
        print("PASS: fullscreen app preserved, actual containers above editor, native palette typing and cancellation")
    except BaseException:
        log.flush()
        log.seek(0)
        print(log.read())
        raise
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)
        log.close()
