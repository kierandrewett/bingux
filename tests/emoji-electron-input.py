#!/usr/bin/env python3
"""Run inside Gnoblin's isolated run-gnome-shell.sh session, never on the host bus."""
import json
import os
from pathlib import Path
import socket
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CONFIG = Path(os.environ.get("XDG_CONFIG_HOME", ""))
assert str(CONFIG).startswith("/tmp/gnoblin-gs."), "Use the isolated Gnoblin test runner"
SOCKET = os.environ["GNOBLIN_COMPOSITOR_SOCKET"]
X11 = os.environ.get("GNOBLIN_TEST_XWAYLAND") == "1"
IPC = ["qs", "ipc", "-p", str(CONFIG / "emoji-test"), "call", "emoji"]


def status():
    return json.loads(subprocess.check_output(IPC + ["status"], text=True, timeout=3))


def wait_for(check, message):
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(.04)
    raise AssertionError(message)


def drive(mode, pid, query):
    from gi.repository import Gio, GLib
    with socket.socket(socket.AF_UNIX) as bridge:
        bridge.settimeout(3)
        bridge.connect(SOCKET)
        stream = bridge.makefile("r")
        def request(record, event):
            bridge.sendall((json.dumps(record) + "\n").encode())
            while True:
                reply = json.loads(stream.readline())
                if reply["event"] == "error" and event != "error":
                    raise AssertionError(reply)
                if reply["event"] == event:
                    return reply
        windows = request({"op": "windows"}, "windows")["windows"]
        window = next(w for w in windows if w["title"] == "Bingux Electron input test")
        bridge.sendall((json.dumps({"op": "activate", "window": window["id"]}) + "\n").encode())
        def anchor():
            reply = request({"op": "input-anchor"}, "input-anchor")
            return reply if reply["pid"] == pid else None
        wait_for(anchor, "Electron did not receive focus")
        if mode == "focus":
            return
        if mode == "blur":
            if X11: return
            wait_for(lambda: (r := anchor()) and r.get("caret") is None, "Stale caret after input blur")
            error = request({"op": "type-text", "window": window["id"], "text": "unexpected"}, "error")
            assert "text input" in error["message"], error
            return
        before = anchor() if X11 else wait_for(lambda: (r if (r := anchor()) and r.get("caret") else None), "No native Electron caret")
        if not X11: assert before["caret"]["height"] > 0, before
        assert not status()["visible"]
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        def key(symbol, down):
            bus.call_sync("org.gnoblin.EmojiTest", "/org/gnoblin/EmojiTest", "org.gnoblin.EmojiTest",
                          "Key", GLib.Variant("(ub)", (symbol, down)), None, Gio.DBusCallFlags.NONE, 3000, None)
        try:
            time.sleep(.1)
            key(0xffeb, True)
            time.sleep(.05)
            key(ord("."), True)
            time.sleep(.05)
            key(ord("."), False)
            key(0xffeb, False)
            try:
                picker = wait_for(lambda: (s if (s := status())["visible"] else None), "Picker did not open")
            except AssertionError:
                print("Picker state:", status(), flush=True)
                raise
            if not X11: assert picker["anchor"] == "caret", picker
            rect = before["caret"] or {"x": -1000, "y": -1000, "width": 1, "height": 1}
            x = picker["x"] + picker["screenX"]
            y = picker["y"] + picker["screenY"]
            assert (y + 408 <= rect["y"] or y >= rect["y"] + rect["height"]
                    or x + 440 <= rect["x"] or x >= rect["x"] + max(1, rect["width"])), (picker, rect)
            time.sleep(.15)
            if query == "thumbs up":
                # Choose medium tone using the same keyboard path as the user.
                for symbol in [0xff09, 0xff0d, 0xff53, 0xff53, 0xff53, 0xff0d]:
                    key(symbol, True); key(symbol, False); time.sleep(.05)
            for char in query:
                key(ord(char), True)
                key(ord(char), False)
            wait_for(lambda: status()["query"] == query, "Picker did not receive search text")
            if os.environ.get("BINGUX_EMOJI_SCREENSHOT") and query == "grinning face":
                subprocess.run(["grim", os.environ["BINGUX_EMOJI_SCREENSHOT"]], check=True, timeout=3)
            key(0xff0d, True)
            key(0xff0d, False)
            wait_for(lambda: not status()["visible"], "Enter did not dismiss picker")
            time.sleep(1.1 if X11 else .5)
            assert not status()["error"], status()
        finally:
            subprocess.run(IPC + ["close"], check=True, timeout=3, capture_output=True)


if sys.argv[1:2] == ["--drive"]:
    drive(sys.argv[2], int(sys.argv[3]), sys.argv[4] if len(sys.argv) > 4 else "")
else:
    scripts = CONFIG / "gnoblin/scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    gnoblin = Path(os.environ["GNOBLIN_SOURCE"])
    shutil.copy2(gnoblin / "src/scripts/compositor-bridge.js", scripts)
    shutil.copytree(gnoblin / "src/scripts/lib", scripts / "lib")
    (scripts / "emoji-test.js").write_text('''import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {CommandShortcuts} from 'CONFIG_URL';
export default function enable(api) {
    const shortcuts = new CommandShortcuts(global.display, (action, enabled) => {
        Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action),
            enabled ? Shell.ActionMode.NORMAL : Shell.ActionMode.NONE);
    });
    shortcuts.apply([{name: 'emoji', binding: '<Super>period', command: EMOJI_COMMAND}]);
    api._disposers.push(() => shortcuts.destroy());
    const seat = global.stage.context.get_backend().get_default_seat();
    const keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
    const pointer = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    pointer.notify_absolute_motion(GLib.get_monotonic_time(), 20, 20);
    const service = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="org.gnoblin.EmojiTest">
        <method name="Display"><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
        <method name="Key"><arg type="u" direction="in"/><arg type="b" direction="in"/></method>
        </interface></node>`, {
        Display() { return [GLib.getenv("DISPLAY") || "", GLib.getenv("XAUTHORITY") || ""]; },
        Key(symbol, down) {
            keyboard.notify_keyval(GLib.get_monotonic_time(), symbol,
                down ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED);
        },
    });
    service.export(Gio.DBus.session, '/org/gnoblin/EmojiTest');
    const name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnoblin.EmojiTest', Gio.BusNameOwnerFlags.NONE, null, null, null);
    api._disposers.push(() => { service.unexport(); Gio.bus_unown_name(name); keyboard.run_dispose(); pointer.run_dispose(); });
}
'''.replace('CONFIG_URL', (gnoblin / 'src/gnome-shell-overlay/js/ui/components/gnoblinConfig.js').as_uri())
        .replace('EMOJI_COMMAND', json.dumps(IPC + ['open'])))
    subprocess.run([str(gnoblin / "src/tools/gnoblinctl"), "reload-scripts"], check=True)
    fixture = CONFIG / "emoji-test"
    fixture.mkdir()
    for path in (ROOT / "shell/bingux").iterdir():
        if path.is_file() and path.name != "shell.qml":
            shutil.copy2(path, fixture / path.name)
    (fixture / "shell.qml").write_text('import Quickshell\nShellRoot { EmojiPicker { persistRecent: false } }\n')
    with (CONFIG / "emoji-shell.log").open("w") as log:
        shell = subprocess.Popen(["qs", "-p", str(fixture)], stdout=log, stderr=log)
        try:
            def ready():
                try:
                    return status()["ready"]
                except (subprocess.SubprocessError, ValueError):
                    return False
            wait_for(ready, "Picker did not start")
            if X11:
                from gi.repository import Gio
                bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
                reply = bus.call_sync("org.gnoblin.EmojiTest", "/org/gnoblin/EmojiTest", "org.gnoblin.EmojiTest",
                                      "Display", None, None, Gio.DBusCallFlags.NONE, 3000, None)
                os.environ["DISPLAY"], os.environ["XAUTHORITY"] = reply.unpack()
            subprocess.run([os.environ["BINGUX_TEST_ELECTRON"], "--ozone-platform=" + ("x11" if X11 else "wayland"),
                            str(ROOT / "tests/emoji-electron-input.cjs")], check=True, timeout=45)
        finally:
            shell.terminate()
            shell.wait(timeout=5)
            Path(SOCKET).unlink(missing_ok=True)
            if shell.returncode not in (0, -15):
                print((CONFIG / "emoji-shell.log").read_text()[-3000:])
    subprocess.run(["bash", str(ROOT / "tests/control-centre.sh")], check=True, timeout=35,
                   env={**os.environ, "BINGUX_CONTROL_TEST_QML": str(ROOT / "tests/emoji-picker.qml")})
