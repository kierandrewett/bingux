#!/usr/bin/env python3
"""Exercise dock action identity and GIO activation on a private D-Bus session."""

import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "shell/bingux"
LAUNCHER = SHELL / "launch-application.py"
ACTION = "--private-mode"
DESKTOP = "org.bingux.ActionFixture"
QML = r"""
const fs=require("fs"), vm=require("vm"), url=require("url");
const shell=process.argv[1], commands=[];
const source=fs.readFileSync(shell+"/DockAppMenu.qml","utf8").split("id: desktopActions")[1];
const handler=source.match(/                    onTriggered: \{\n(.*?)^                    \}/ms);
if(!handler) throw new Error("Desktop action handler not found");
const context={
    modelData:{id:process.argv[2],name:"Fenêtre privée",execute:()=>commands.push(["DIRECT_EXECUTE"])},
    button:{menuOpen:true,currentGroup:{desktopEntry:{id:"org.bingux.ActionFixture"}}},
    Quickshell:{env:()=>"",execDetached:argv=>commands.push(Array.from(argv))},
    Qt:{resolvedUrl:name=>url.pathToFileURL(shell+"/"+name)}
};
vm.createContext(context);
vm.runInContext("(function(){"+handler[1]+"})()",context);
console.log(JSON.stringify({commands,menuOpen:context.button.menuOpen}));
"""
INTERFACE = """
<node><interface name="org.freedesktop.Application">
<method name="ActivateAction">
  <arg type="s" direction="in"/>
  <arg type="av" direction="in"/>
  <arg type="a{sv}" direction="in"/>
</method>
</interface></node>
"""


def main():
    # GIO must never contact the host session bus, even for a rejected action.
    if "--private-bus" not in sys.argv:
        env = dict(os.environ)
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(name, None)
        return subprocess.run(
            ["dbus-run-session", "--", sys.executable, __file__, "--private-bus"], env=env, timeout=30
        ).returncode
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    fixture = runpy.run_path(str(ROOT / "tests/direct-app-launch.py"))
    failures = []
    with tempfile.TemporaryDirectory(prefix="bingux-dock-action-") as temporary:
        root = Path(temporary)
        (root / "child.c").write_text(fixture["CHILD"])
        subprocess.run(["cc", "-O2", "-Wall", "-Werror", str(root / "child.c"), "-o", str(root / "child")], check=True)
        apps = root / "applications"
        apps.mkdir()
        working = root / "working directory"
        working.mkdir()
        record = root / "record.jsonl"
        desktop = apps / f"{DESKTOP}.desktop"
        desktop.write_text(
            f"[Desktop Entry]\nType=Application\nName=Fixture app\nPath={working}\n"
            f"Exec={root / 'child'} default\nActions={ACTION};new-window;\n"
            f"[Desktop Action {ACTION}]\nName=Fenêtre privée\n"
            f'Exec={root / "child"} {ACTION} %k %c %% "literal ;$(touch forbidden)"\n'
            f"[Desktop Action new-window]\nName=New window\nExec={root / 'child'} new-window\n"
        )
        binaries = root / "bin"
        binaries.mkdir()
        for name in ("systemctl", "systemd-run"):
            path = binaries / name
            path.write_text("#!/bin/sh\nexit " + ("1" if name == "systemd-run" else "0") + "\n")
            path.chmod(0o700)
        env = os.environ | {
            "XDG_DATA_HOME": str(root),
            "XDG_DATA_DIRS": str(root),
            "PATH": str(binaries) + os.pathsep + os.environ["PATH"],
            "BINGUX_TEST_RECORD": str(record),
            "BINGUX_TEST_VALUE": "unchanged",
        }
        output = subprocess.run(["node", "-e", QML, str(SHELL), ACTION], capture_output=True, text=True, check=True)
        dispatch = json.loads(output.stdout)
        assert dispatch["menuOpen"] is False
        command = dispatch["commands"][0]
        if f"--action={ACTION}" not in command or command[-1] != DESKTOP:
            failures.append(f"QML did not dispatch the action ID and desktop ID through GIO: {command}")

        def launch(arguments):
            record.unlink(missing_ok=True)
            return subprocess.run(
                [sys.executable, str(LAUNCHER), *arguments], env=env, capture_output=True, text=True, timeout=7
            )

        for flags, expected in (([f"--action={ACTION}"], ACTION), (["--new-window"], "new-window")):
            result = launch(["--host-launch", *flags, "--", DESKTOP])
            if result.returncode:
                failures.append(f"{flags}: exit {result.returncode}: {result.stderr.strip()}")
                continue
            row = fixture["records"](record, 1)[0]
            expected_argv = (
                [expected]
                if expected == "new-window"
                else [ACTION, str(desktop), "Fixture app", "%", "literal ;$(touch forbidden)"]
            )
            assert row["argv"] == expected_argv, row
            assert row["cwd"] == str(working), row
            assert not (working / "forbidden").exists()
            print("GIO exec:", json.dumps(row))

        if "--action" in command or f"--action={ACTION}" in command:
            record.unlink(missing_ok=True)
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=7)
            assert result.returncode == 0, result.stderr
            row = fixture["records"](record, 1)[0]
            assert row["argv"][0] == ACTION, row
            print("QML manager-rejection fallback:", json.dumps(row))

        for invalid in ("Fenêtre privée", "not-an-action", ""):
            result = launch(["--host-launch", f"--action={invalid}", "--", DESKTOP])
            if result.returncode != 1 or "desktop action" not in result.stderr or record.exists():
                failures.append(
                    f"invalid action {invalid!r}: expected explicit rejection without default launch, got {result.returncode}"
                )
        conflict = launch([f"--action={ACTION}", "--new-window", "--", DESKTOP])
        assert conflict.returncode == 2 and not record.exists()

        # A running DBusActivatable application must receive ActivateAction,
        # not its desktop Exec command or an action's localized display label.
        name = "org.bingux.ActivationFixture"
        (apps / f"{name}.desktop").write_text(
            f"[Desktop Entry]\nType=Application\nName=Activation fixture\nDBusActivatable=true\n"
            f"Exec={root / 'child'} WRONG_DEFAULT\nActions={ACTION};\n"
            f"[Desktop Action {ACTION}]\nName=Fenêtre privée\nExec={root / 'child'} WRONG_EXEC\n"
        )
        connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        connection.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            GLib.Variant("(su)", (name, 0)),
            None,
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        )
        calls = []

        def called(_connection, _sender, _path, _interface, method, parameters, invocation):
            calls.append({"method": method, "action": parameters.unpack()[0]})
            invocation.return_value(GLib.Variant("()", ()))

        registration = connection.register_object(
            "/org/bingux/ActivationFixture", Gio.DBusNodeInfo.new_for_xml(INTERFACE).interfaces[0], called, None, None
        )
        record.unlink(missing_ok=True)
        process = subprocess.Popen(
            [sys.executable, str(LAUNCHER), "--host-launch", f"--action={ACTION}", "--", name],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 6
            while time.monotonic() < deadline and (process.poll() is None or not calls):
                GLib.MainContext.default().iteration(False)
                if process.poll() not in (None, 0):
                    break
                time.sleep(0.005)
            stdout, stderr = process.communicate(timeout=1)
            if process.returncode or calls != [{"method": "ActivateAction", "action": ACTION}] or record.exists():
                failures.append(f"D-Bus activation: code={process.returncode}, calls={calls}, stderr={stderr.strip()}")
            print("Private D-Bus:", json.dumps(calls))
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()
            connection.unregister_object(registration)
        print(
            "Source hashes:",
            json.dumps(
                {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [LAUNCHER, SHELL / "DockAppMenu.qml"]}
            ),
        )
        if failures:
            print("FAIL:\n" + "\n".join(failures), file=sys.stderr)
            return 1
        print(
            "PASS: action ID; GIO field codes/cwd; new-window; manager rejection; invalid action; private D-Bus ActivateAction"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
