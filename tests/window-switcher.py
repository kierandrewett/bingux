#!/usr/bin/env python3
"""Exercise the switcher with virtual keyboard events in a private session."""

import ast
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
assert os.environ.get("GNOBLIN_COMPOSITOR_SOCKET", "").startswith("/tmp/"), "Use a private test compositor socket"
bingux = Path(__file__).resolve().parent.parent
repo = bingux.parent / "gnoblin"
qs = os.environ.get("QUICKSHELL_BIN", "qs")
scripts = Path(os.environ["XDG_CONFIG_HOME"]) / "gnoblin/scripts"
scripts.mkdir(parents=True, exist_ok=True)
shutil.copy2(repo / "src/scripts/compositor-bridge.js", scripts)
shutil.copytree(repo / "src/scripts/lib", scripts / "lib", dirs_exist_ok=True)
(scripts / "switcher-test.js").write_text("""
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
export default function enable(api) {
 // A physical keyboard survives shell script reloads. Keep this private test
 // device for the session too, so reload exercises the switcher's own cleanup.
 global.__switcherTestKeyboard ??= global.stage.context.get_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
 const device = global.__switcherTestKeyboard;
 global.__switcherTestPointer ??= global.stage.context.get_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
 global.__switcherTestPointer.notify_absolute_motion(GLib.get_monotonic_time(), 20, 20);
 const settings = new Gio.Settings({schema_id: 'org.gnome.desktop.wm.keybindings'});
 for (const key of ['switch-applications', 'switch-applications-backward', 'switch-windows', 'switch-windows-backward']) settings.set_strv(key, []);
 let released = 0, latency = 0;
 const focus = global.display.connect('notify::focus-window', () => {
  if (released && global.display.focus_window) latency = GLib.get_monotonic_time() - released;
 });
 const impl = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="org.gnoblin.SwitcherTest">
 <method name="Key"><arg type="u" direction="in"/><arg type="b" direction="in"/></method>
 <method name="State"><arg type="s" direction="out"/></method>
 <method name="Focus"><arg type="s" direction="in"/></method>
 <method name="Minimize"><arg type="s" direction="in"/></method>
 <method name="Click"><arg type="u" direction="in"/><arg type="u" direction="in"/></method>
 </interface></node>`, {
  Key(code, down) {
   if (code === 56 && !down) { released = GLib.get_monotonic_time(); latency = 0; }
   device.notify_key(GLib.get_monotonic_time(), code, down ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED);
  },
  Focus(title) {
   const window = global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).find(w => w.title === title);
   Main.activateWindow(window, global.get_current_time());
  },
  Minimize(title) {
   global.display.list_all_windows().find(window => window.title === title).minimize();
  },
  Click(x, y) {
   const pointer = global.__switcherTestPointer;
   pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
   pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED);
   pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED);
  },
  State() {
   return JSON.stringify({focus: global.display.focus_window?.title ?? null, latency,
    windows: global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).map(w => w.title)});
  },
 });
 impl.export(Gio.DBus.session, '/org/gnoblin/SwitcherTest');
 const name = Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnoblin.SwitcherTest', Gio.BusNameOwnerFlags.NONE, null, null, null);
 api._disposers.push(() => {
  global.display.disconnect(focus); impl.unexport(); Gio.bus_unown_name(name);
 });
}
""")


def reload():
    subprocess.run([str(repo / "src/tools/gnoblinctl"), "script", "reload"], check=True)


def call(method, *args):
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.gnoblin.SwitcherTest",
            "--object-path",
            "/org/gnoblin/SwitcherTest",
            "--method",
            "org.gnoblin.SwitcherTest." + method,
            *map(str, args),
        ],
        check=True,
        text=True,
        capture_output=True,
    )
    return ast.literal_eval(result.stdout)


def state():
    native = json.loads(call("State")[0])
    output = subprocess.run(
        [qs, "ipc", "-p", str(qml), "call", "switcher", "status"], check=True, capture_output=True, text=True
    ).stdout
    switcher = json.loads(output)
    return {
        **native,
        "shown": switcher["shown"],
        "visible": switcher["active"],
        "label": switcher["selected"] or "",
        "ready": switcher["ready"],
        "previewCount": switcher["previewCount"],
        "previewRequests": switcher["previewRequests"],
        "previewError": switcher["previewError"],
    }


def wait_for(predicate, timeout=3):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = state()
        if predicate(value):
            return value
        time.sleep(0.01)
    raise AssertionError(value)


def key(code, down):
    call("Key", code, "true" if down else "false")


def tap(code):
    key(code, True)
    key(code, False)


qml = scripts.parent / "switcher.qml"
qml.write_text(
    'import Quickshell\nimport "' + (bingux / "shell/bingux").as_uri() + '"\nShellRoot { WindowSwitcher {} }'
)
config = Path(os.environ["XDG_CONFIG_HOME"]) / "bingux/switcher.json"
config.parent.mkdir(parents=True, exist_ok=True)
config.write_text("{}")
compositor_config = Path(os.environ["XDG_CONFIG_HOME"]) / "gnoblin/init.lua"
compositor_config.write_text(
    (compositor_config.read_text() if compositor_config.exists() else "")
    + """
local g = require("gnoblin")
g.set({["window-rules"] = {{
    match = {layer = "^bingux-switcher$"},
    animation = "none",
    opacity = 1.0,
    ["blur-ignore-shadows"] = true,
    blur = 24,
}}})
"""
)
subprocess.run([str(repo / "src/tools/gnoblinctl"), "config", "reload"], check=True)
reload()
qslog = open("/tmp/bingux-switcher-runtime.log", "w")
shell = subprocess.Popen([qs, "-p", str(qml)], stdout=qslog, stderr=qslog)
time.sleep(0.7)
wait_for(lambda s: s["ready"])
apps = []
try:
    for title in ["Switcher One", "Switcher Two", "Switcher Three"]:
        apps.append(
            subprocess.Popen(
                [
                    "foot",
                    "--app-id=gnoblin-switcher-test",
                    "--title=" + title,
                    "sh",
                    "-c",
                    'printf "\\033[44m Preview content \\033[0m\\n"; sleep 90',
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        )
        wait_for(lambda s: s["focus"] == title)
    assert state()["windows"][:3] == ["Switcher Three", "Switcher Two", "Switcher One"]
    call("Minimize", "Switcher One")
    key(56, True)
    tap(15)
    shown = wait_for(lambda s: s["shown"])
    wait_for(lambda s: s["previewCount"] == 3)
    captured = state()["previewRequests"]
    time.sleep(0.65)
    assert state()["previewRequests"] == captured == 3, (
        "Previews must stop capturing once the visible windows are cached"
    )
    tap(1)
    key(56, False)
    wait_for(lambda s: not s["visible"])
    time.sleep(2.1)
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    time.sleep(0.7)
    assert state()["previewCount"] == 3, "Reopening must retain every cached preview"
    assert state()["previewRequests"] == captured + 1, "Only the focused window needs a fresh capture"
    # Inspect actual PNG content, not just a successful preview response.
    import socket
    import base64
    from PIL import Image

    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(3)
        connection.connect(os.environ["GNOBLIN_COMPOSITOR_SOCKET"])
        transport = connection.makefile("rwb", buffering=0)
        json.loads(transport.readline())
        transport.write(b'{"op":"windows"}\n')
        snapshot = json.loads(transport.readline())["windows"]
        for window in snapshot:
            transport.write(
                (json.dumps({"op": "preview", "window": window["id"], "width": 224, "height": 126}) + "\n").encode()
            )
            response = json.loads(transport.readline())
            data = base64.b64decode(response["source"].split(",")[1])
            path = Path("/tmp/bingux-preview-" + window["id"] + ".png")
            path.write_bytes(data)
            image = Image.open(path).convert("RGBA")
            assert image.getextrema()[3][1] > 0, ("Preview must contain visible pixels", window)
            assert len(image.getcolors(image.width * image.height)) > 2, ("Preview must contain window content", window)
    subprocess.run(["grim", "/tmp/gnoblin-switcher-preview.png"], check=True)
    assert shown["focus"] == "Switcher Three", shown
    assert shown["label"].startswith("Switcher Two"), shown
    tap(15)
    wait_for(lambda s: s["label"].startswith("Switcher One"))
    key(56, False)
    wait_for(lambda s: s["focus"] == "Switcher One" and not s["visible"])
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    tap(1)
    key(56, False)
    cancelled = wait_for(lambda s: not s["visible"])
    assert cancelled["focus"] == "Switcher One", ("Escape must not change focus", cancelled)
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    call("Click", 20, 20)
    wait_for(lambda s: not s["visible"])
    key(56, False)
    assert state()["focus"] == "Switcher One", "outside click must cancel without changing focus"
    key(56, True)
    key(42, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    assert state()["label"].startswith("Switcher Two"), state()
    key(42, False)
    key(56, False)
    wait_for(lambda s: s["focus"] == "Switcher Two")
    for _ in range(10):
        previous = state()["focus"]
        key(56, True)
        tap(15)
        wait_for(lambda s: s["shown"])
        tap(1)
        key(56, False)
        cancelled = wait_for(lambda s: not s["visible"])
        assert cancelled["focus"] == previous, ("rapid cancellation changed focus", cancelled)
    timings = []
    for _ in range(20):
        previous = state()["focus"]
        key(56, True)
        tap(15)
        key(56, False)
        result = wait_for(lambda s: s["focus"] != previous and not s["visible"])
        assert result["latency"] > 0, result
        timings.append(result["latency"] / 1000)
    print(f"KEY RELEASE TO FOCUS: median {statistics.median(timings):.2f} ms; max {max(timings):.2f} ms (20 switches)")
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    apps[0].terminate()
    apps[0].wait(timeout=3)
    wait_for(lambda s: "Switcher One" not in s["windows"])
    key(56, False)
    wait_for(lambda s: not s["visible"] and s["focus"] in ["Switcher Two", "Switcher Three"])
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    reload()
    key(56, False)
    wait_for(lambda s: s["ready"] and not s["visible"])
    previous = state()["focus"]
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    shell.terminate()
    shell.wait(timeout=5)
    shell = subprocess.Popen([qs, "-p", str(qml)], stdout=qslog, stderr=qslog)
    time.sleep(0.7)
    wait_for(lambda s: s["ready"])
    key(56, False)
    wait_for(lambda s: s["focus"] != previous and not s["visible"])
    assert not state()["visible"], "Restarted UI must not retain the fallback gesture"
    config = Path(os.environ["XDG_CONFIG_HOME"]) / "bingux/switcher.json"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text('{"enabled":false}')
    time.sleep(0.3)
    key(56, True)
    tap(15)
    time.sleep(0.15)
    assert not state()["visible"], "disabled switcher still took the binding"
    key(56, False)
    config.write_text('{"showDelay":1}')
    time.sleep(0.3)
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    tap(1)
    key(56, False)
    config.write_text('{"showDelay":-1}')
    time.sleep(0.3)
    key(56, True)
    tap(15)
    wait_for(lambda s: s["shown"])
    tap(1)
    key(56, False)
    print(
        "PASS: real Alt+Tab, stable MRU, reverse, Escape, outside click, rapid switching, closed windows, script/config reload, Quickshell restart and invalid config recovery"
    )
finally:
    shell.terminate()
    shell.wait(timeout=5)
    qslog.close()
    for app in apps:
        if app.poll() is None:
            app.terminate()
            app.wait(timeout=3)
