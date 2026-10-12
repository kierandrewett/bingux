from pathlib import Path
import argparse
import subprocess
import json
import time
import socket
import shutil
import tempfile

REPO = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description="Run dock-folder interaction checks in an existing Gnoblin QEMU guest.")
parser.add_argument("--run", type=Path, required=True, help="existing qemu-e2e run directory")
parser.add_argument("--source", type=Path, default=REPO / "shell/bingux/Dock.qml", help="Dock.qml source")
parser.add_argument(
    "--gnoblin-repo", type=Path, default=REPO.parent / "gnoblin", help="Gnoblin checkout containing scripts/qemu-e2e"
)
parser.add_argument("--output", type=Path, required=True, help="new artifact directory")
parser.add_argument(
    "--allow-eager", action="store_true", help="allow eager popup state for original-baseline comparison"
)
args = parser.parse_args()
run = args.run.resolve()
runner = str(args.gnoblin_repo.resolve() / "scripts/qemu-e2e")
source = args.source.resolve()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=False)
label = "baseline" if args.allow_eager else "candidate"


def guest(*a, data=None):
    r = subprocess.run([runner, "guest", str(run), "--", *a], input=data, capture_output=True, timeout=30)
    r.check_returncode()
    return r.stdout.decode()


fixture = Path(tempfile.mkdtemp(prefix="dock-folder-fixture-"))
repo = REPO
for item in (repo / "shell/bingux").iterdir():
    if item.suffix in (".qml", ".js") or item.name == "qmldir":
        shutil.copy2(item, fixture / item.name)
shutil.copytree(repo / "shell/bingux/icons", fixture / "icons", dirs_exist_ok=True)
dock = source.read_text().replace("ToplevelManager", "root.testManager")
dock = dock.replace(
    "required property var settings",
    "required property var settings\n    required property var testManager\n    property alias testItems: dockItems\n    readonly property var testFolderPopup: folderPopup",
)
dock = dock.replace("model: root.testManager.toplevels", "model: root.testManager.toplevels.values")
(fixture / "Dock.qml").write_text(dock)
(fixture / "shell.qml").write_text((REPO / "tests/dock-folder-native.qml").read_text())
prefs = (fixture / "BinguxPreferences.qml").read_text()
prefs = (
    prefs[: prefs.index("    readonly property string path:")]
    + """    property bool loaded: true
    function saveDesktop(changes) {data=Object.assign({},data,{desktop:Object.assign({},data.desktop,changes)});}
}
"""
)
(fixture / "BinguxPreferences.qml").write_text(prefs)
data = subprocess.check_output(["tar", "-C", str(fixture), "-cf", "-", "."])
shutil.rmtree(fixture)
account = json.loads(
    guest(
        "python3",
        "-c",
        "import json,os,pwd;u=pwd.getpwuid(os.getuid());print(json.dumps({'home':u.pw_dir,'uid':u.pw_uid}))",
    )
)
home, uid = account["home"], account["uid"]
monitor_env = [
    "env",
    f"XDG_RUNTIME_DIR=/run/user/{uid}",
    f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus",
    f"PATH=/usr/bin:{home}/.local/share/bingux/bin:/usr/local/lib/gnoblin/bin:/bin",
]
monitors = json.loads(guest(*monitor_env, "gnoblinctl", "--json", "monitor", "list"))["monitors"]
if len(monitors) != 1 or monitors[0]["scale"] != 1 or monitors[0]["transform"] != "normal":
    raise RuntimeError(f"requires one untransformed scale-1 monitor; got {monitors!r}")
monitor_width, monitor_height = monitors[0]["width"], monitors[0]["height"]
target = home + "/folder-" + label
guest(
    "python3",
    "-c",
    "import sys,tarfile,io;from pathlib import Path;p=Path(sys.argv[1]);p.mkdir(exist_ok=True);tarfile.open(fileobj=io.BytesIO(sys.stdin.buffer.read())).extractall(p,filter='data')",
    target,
    data=data,
)
env = [
    "env",
    f"XDG_RUNTIME_DIR=/run/user/{uid}",
    "WAYLAND_DISPLAY=wayland-0",
    f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus",
    f"HOME={home}",
    f"PATH=/usr/bin:{home}/.local/share/bingux/bin:{home}/.local/share/gnoblin/quickshell-fedora/bin:/usr/local/lib/gnoblin/bin:/bin",
    f"QML_IMPORT_PATH={home}/.local/share/bingux/lib/bingux/qml",
]
qs = home + "/.local/share/gnoblin/quickshell-fedora/bin/quickshell"
launch = "import subprocess,os,sys;from pathlib import Path;p=subprocess.Popen(sys.argv[1:],stdout=open(Path.home()/'folder-run.log','w'),stderr=subprocess.STDOUT,start_new_session=True);print(p.pid)"
pid = int(guest(*env, "python3", "-c", launch, qs, "-p", target, "--no-color"))
s = socket.socket(socket.AF_UNIX)
s.settimeout(10)
s.connect(str(run / "qmp.sock"))
f = s.makefile("rw")
f.readline()


def qmp(cmd, **a):
    f.write(json.dumps({"execute": cmd, "arguments": a}) + "\n")
    f.flush()
    while True:
        r = json.loads(f.readline())
        if "error" in r:
            raise RuntimeError(r)
        if "return" in r:
            return r["return"]


qmp("qmp_capabilities")


def ipc(name, *a):
    t = guest(*env, qs, "-p", target, "ipc", "call", "folder-test", name, *a).strip()
    return json.loads(t) if t else None


def info():
    return ipc("status")


def poll(pred, t=8):
    end = time.monotonic() + t
    v = None
    while time.monotonic() < end:
        try:
            v = info()
            if pred(v):
                return v
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            pass
        time.sleep(0.08)
    raise AssertionError(("timeout", v))


def pointer(x, y, down=None):
    events = [
        {"type": "abs", "data": {"axis": "x", "value": round(x * 32767 / (monitor_width - 1))}},
        {"type": "abs", "data": {"axis": "y", "value": round(y * 32767 / (monitor_height - 1))}},
    ]
    if down is not None:
        events.append({"type": "btn", "data": {"button": "left", "down": down}})
    qmp("input-send-event", events=events)


def click(b):
    pointer(b["x"], b["y"], True)
    pointer(b["x"], b["y"], False)


rows = []
try:
    cold = poll(lambda v: bool(v["generation"]))
    rows.append({"cold": cold})
    assert cold["popup"] is None or args.allow_eager, ("Cold popup unexpectedly exists", cold)
    ipc("invalid")
    assert info()["popup"] is None or args.allow_eager
    ipc("seed", "saved")
    saved = poll(lambda v: any(b["id"] == "folder:fixture" for b in v["buttons"]))
    time.sleep(0.5)
    saved = info()
    rows.append({"saved_closed": saved})
    assert saved["popup"] is None or args.allow_eager
    ipc("coldDrop")
    cold_drop = poll(lambda v: len(v["folders"][0]["apps"]) == 11 and not v["dragged"])
    rows.append({"cold_drop": cold_drop})
    assert cold_drop["popup"] is None or args.allow_eager
    click(next(b for b in info()["buttons"] if b["id"] == "folder:fixture"))
    first = poll(lambda v: v["popup"] and v["popup"]["visible"])
    assert first["popup"]["sameScreen"] and first["popup"]["anchor"] and first["popup"]["pageCount"] == 2, first
    time.sleep(0.4)
    qmp("screendump", filename=str(out / "first.ppm"))
    rows.append({"first": info()})
    pointer(10, 400, True)
    try:
        hidden = poll(lambda v: not v["popup"]["visible"])
    finally:
        pointer(10, 400, False)
    assert hidden["popup"]["identity"] == first["popup"]["identity"]
    poll(lambda v: v["popup"]["opacity"] == 0 and not v["popup"]["flight"])
    click(next(b for b in info()["buttons"] if b["id"] == "folder:fixture"))
    reopen = poll(lambda v: v["popup"]["visible"])
    assert reopen["popup"]["identity"] == first["popup"]["identity"]
    qmp("send-key", keys=[{"type": "qcode", "data": "esc"}])
    poll(lambda v: not v["popup"]["visible"])
    poll(lambda v: v["popup"]["opacity"] == 0 and not v["popup"]["flight"])
    # Actual hover-drag into folder and page switching.
    before = info()
    app = next(b for b in before["buttons"] if b["id"] != "folder:fixture" and b["launchable"])
    folder = next(b for b in before["buttons"] if b["id"] == "folder:fixture")
    pointer(app["x"], app["y"], True)
    try:
        for i in range(1, 9):
            pointer(app["x"] + (folder["x"] - app["x"]) * i / 8, app["y"] - 10 * i / 8)
            time.sleep(0.05)
        hover = poll(lambda v: v["dragged"] and v["popup"]["visible"])
        rows.append({"hover": hover})
        p = hover["popup"]
        # Keep the actual pointer pressed while it leaves the dock surface.
        for i in range(1, 13):
            pointer(
                folder["x"] + (p["x"] + p["width"] - 10 - folder["x"]) * i / 12,
                folder["y"] + (p["y"] + 100 - folder["y"]) * i / 12,
            )
            time.sleep(0.05)
        paged = poll(lambda v: v["popup"]["page"] == 1 and v["drop"] == "folder:fixture")
        rows.append({"native_paged": paged})
        pointer(p["x"] + 70, p["y"] + 130)
        drop = poll(lambda v: v["drop"] == "folder:fixture")
        rows.append({"native_drop": drop})
    finally:
        pointer(p["x"] + 70, p["y"] + 130, False)
    added = poll(lambda v: len(v["folders"][0]["apps"]) == 12 and not v["dragged"])
    rows.append({"added": added})
    assert added["popup"]["identity"] == first["popup"]["identity"]
    ipc("remove")
    removed = poll(lambda v: not v["folders"] and not v["popup"]["visible"])
    rows.append({"removed": removed})
    # Real drag-to-create a new folder after a cold engine reload.
    ipc("seed", "apps")
    ipc("reload")
    new = poll(lambda v: v["generation"] != cold["generation"])
    assert new["popup"] is None or args.allow_eager
    ipc("seed", "apps")
    apps = poll(lambda v: len(v["buttons"]) > 2)
    time.sleep(0.5)
    apps = info()["buttons"]
    a, b = apps[0], apps[1]
    pointer(a["x"], a["y"], True)
    try:
        for i in range(1, 9):
            pointer(a["x"] + (b["x"] - a["x"]) * i / 8, a["y"] - 10 * i / 8)
            time.sleep(0.05)
        poll(lambda v: v["ready"] and v["hover"] == b["id"])
    finally:
        pointer(b["x"], b["y"] - 10, False)
    created = poll(lambda v: len(v["folders"]) == 1 and v["popup"] and v["popup"]["visible"])
    rows.append({"created": created})
    assert len(created["folders"][0]["apps"]) == 2
    qmp("screendump", filename=str(out / "created.ppm"))
    # Open/closing/hidden reload must discard the old item and restore cold state.
    for condition in ("open", "closing", "hidden"):
        if condition != "open":
            ipc("close")
        if condition == "hidden":
            poll(lambda v: v["popup"]["opacity"] == 0 and not v["popup"]["flight"])
        before = info()
        ipc("reload")
        fresh = poll(lambda v: v["generation"] != before["generation"])
        assert fresh["popup"] is None or args.allow_eager
        rows.append({"reload": condition, "state": fresh})
        ipc("seed", "saved")
        v = poll(lambda v: any(b["id"] == "folder:fixture" for b in v["buttons"]))
        time.sleep(0.3)
        click(next(b for b in info()["buttons"] if b["id"] == "folder:fixture"))
        poll(lambda v: v["popup"] and v["popup"]["visible"])
        time.sleep(0.3)
    (out / "result.json").write_text(json.dumps({"ok": True, "records": rows}, indent=2))
    print(json.dumps({"ok": True, "out": str(out)}))
finally:
    try:
        qmp("input-send-event", events=[{"type": "btn", "data": {"button": "left", "down": False}}])
    except Exception:
        pass
    try:
        guest("kill", "-TERM", str(pid))
        log = guest("cat", home + "/folder-run.log")
        (out / "runtime.log").write_text(log)
        (out / "records.json").write_text(json.dumps(rows, indent=2))
    finally:
        f.close()
        s.close()
