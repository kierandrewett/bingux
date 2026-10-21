#!/usr/bin/env python3
"""Drive real VM input; restore the installed main source and restart only guest GDM."""

from pathlib import Path
import argparse
import json
import socket
import subprocess
import threading
import time

REPO = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(
    description="Exercise installed Bingux popup loading and reloads in a disposable native VM."
)
parser.add_argument("--run", required=True, type=Path, help="existing Gnoblin qemu-e2e run")
parser.add_argument("--source", type=Path, default=REPO / "shell/bingux/shell.qml")
parser.add_argument("--gnoblin-repo", type=Path, default=REPO.parent / "gnoblin")
parser.add_argument("--shell-target", help="configured main shell.qml inside the guest")
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
RUN = args.run.resolve()
ROOT = args.gnoblin_repo.resolve()
output = args.output.resolve()
assert (RUN / "qmp.sock").is_socket(), "The QEMU run must be live"
output.mkdir(parents=True, exist_ok=False)


def guest(*argv, data=None, check=True):
    return subprocess.run(
        [str(ROOT / "scripts/qemu-e2e"), "guest", str(RUN), "--", *argv], input=data, capture_output=True, check=check
    ).stdout.decode()


identity = json.loads(
    guest(
        "python3",
        "-c",
        "from pathlib import Path;import json,os;print(json.dumps({'home':str(Path.home()),'uid':os.getuid()}))",
    )
)
GUEST_HOME = identity["home"]
runtime = f"/run/user/{identity['uid']}"
env = [
    "env",
    f"XDG_RUNTIME_DIR={runtime}",
    f"DBUS_SESSION_BUS_ADDRESS=unix:path={runtime}/bus",
    f"PATH=/usr/bin:{GUEST_HOME}/.local/share/bingux/bin:{GUEST_HOME}/.local/share/gnoblin/quickshell-fedora/bin:/usr/local/lib/gnoblin/bin:/bin",
]


def ipc(target, method, *values):
    text = guest(*env, f"{GUEST_HOME}/.local/share/bingux/bin/binguxctl", "ipc", target, method, *values)
    return json.loads(text) if text.strip() else None


def info():
    return ipc("popup-test", "status")


def layers():
    return json.loads(guest(*env, "gnoblinctl", "--json", "layer", "list"))["layers"]


def poll(call, predicate, timeout=8):
    deadline = time.monotonic() + timeout
    latest = None
    while time.monotonic() < deadline:
        try:
            latest = call()
            if predicate(latest):
                return latest
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    raise AssertionError(("poll timeout", latest))


monitors = json.loads(guest(*env, "gnoblinctl", "--json", "monitor", "list"))["monitors"]
assert len(monitors) == 1 and monitors[0]["scale"] == 1 and monitors[0]["transform"] == "normal", (
    "Native input fixture requires one untransformed output at scale 1"
)
display_width = monitors[0]["width"]
display_height = monitors[0]["height"]

sock = socket.socket(socket.AF_UNIX)
sock.settimeout(10)
sock.connect(str(RUN / "qmp.sock"))
stream = sock.makefile("rw")
stream.readline()


def qmp(command, **arguments):
    stream.write(json.dumps({"execute": command, "arguments": arguments}) + "\n")
    stream.flush()
    while True:
        response = json.loads(stream.readline())
        if "error" in response:
            raise RuntimeError(response)
        if "return" in response:
            return response["return"]


qmp("qmp_capabilities")


def pointer(x, y, button="left", pressed=None):
    events = [
        {"type": "abs", "data": {"axis": "x", "value": round(x * 32767 / (display_width - 1))}},
        {"type": "abs", "data": {"axis": "y", "value": round(y * 32767 / (display_height - 1))}},
    ]
    if pressed is not None:
        events.append({"type": "btn", "data": {"button": button, "down": pressed}})
    qmp("input-send-event", events=events)


def click(point, button="left"):
    pointer(point["x"], point["y"], button, True)
    pointer(point["x"], point["y"], button, False)


def escape():
    qmp("send-key", keys=[{"type": "qcode", "data": "esc"}])


# The installed shell runs from the archive that bingux-shell unpacks into the
# guest user's cache; the binary reports that directory.
target = args.shell_target or (
    guest(f"{GUEST_HOME}/.local/share/bingux/bin/bingux-shell", "--shell-dir").strip() + "/shell.qml"
)
original = guest("cat", target).encode()
source = args.source.read_text()
fixture = (REPO / "tests/popups-load-native.inc.qml").read_text()
source = source.rstrip()[:-1] + fixture + "\n}\n"
write = "import sys;from pathlib import Path;Path(sys.argv[1]).write_bytes(sys.stdin.buffer.read())"
records = []
samples = []
stop = threading.Event()
worker = None


def observe():
    while not stop.is_set():
        started = time.monotonic()
        try:
            rows = layers()
            samples.append({"monotonic": started, "layers": rows})
        except Exception as error:
            samples.append({"monotonic": started, "error": str(error)})
        stop.wait(0.05)


try:
    guest("python3", "-c", write, target, data=source.encode())
    guest("sudo", "-n", "systemctl", "restart", "gdm")
    cold = poll(info, lambda value: bool(value.get("generation")))
    assert all(cold[k] is None for k in ("calendar", "metrics", "notifications")), cold
    for name in ("calendar", "metrics", "notifications"):
        assert ipc("shell", "panel", name, "close")["ok"]
    assert all(info()[k] is None for k in ("calendar", "metrics", "notifications"))
    worker = threading.Thread(target=observe)
    worker.start()
    for name, button in [("calendar", "clock"), ("metrics", "metricsButton"), ("notifications", "notificationButton")]:
        stage = {"panel": name, "cold": info()}
        assert stage["cold"][name] is None
        if name == "metrics" and stage["cold"]["metricsOverflow"]:
            click(stage["cold"]["overflowButton"])
            ready = poll(info, lambda value: value["overflowOpen"] and value["metricsButton"]["visible"])
        else:
            ready = poll(info, lambda value: value[button]["visible"])
        click(ready[button])
        first = poll(info, lambda value: value[name] is not None and value[name]["visible"])
        assert (
            first[name]["sameScreen"]
            and first["calendarAnchor"]
            and first["metricsBindings"]
            and first["notificationBindings"]
            and first["notificationLink"]
        ), first
        assert sum(bool(first[k] and first[k]["visible"]) for k in ("calendar", "metrics", "notifications")) == 1
        stage["first"] = first
        if name == "calendar":
            poll(
                layers,
                lambda rows: any(
                    row["namespace"] == "gnoblin-shell-popup" and row["keyboard_interactive"] == "exclusive"
                    for row in rows
                ),
            )
            qmp("send-key", keys=[{"type": "qcode", "data": "right"}])
            stage["first_key"] = poll(info, lambda value: value["calendarDate"] != first["calendarDate"])
        time.sleep(0.3)
        qmp("screendump", filename=str(output / (name + "-first.ppm")))
        escape()
        hidden = poll(info, lambda value: not value[name]["visible"])
        assert hidden[name]["identity"] == first[name]["identity"]
        assert ipc("shell", "panel", name, "open")["ok"]
        reopened = poll(info, lambda value: value[name]["visible"])
        assert reopened[name]["identity"] == first[name]["identity"]
        time.sleep(0.25)
        pointer(10, 400, pressed=True)
        try:
            stage["outside_down"] = poll(info, lambda value: not value[name]["visible"])
        finally:
            pointer(10, 400, pressed=False)
        for condition in ("open", "closing", "hidden"):
            ipc("shell", "panel", name, "open")
            before = poll(info, lambda value: value[name]["visible"])
            old_layers = {row["id"] for row in layers() if row["namespace"] == "gnoblin-shell-popup"}
            if condition == "closing":
                result = ipc("popup-test", "reloadAfterClose", name)
                assert result["closing"], result
            else:
                if condition == "hidden":
                    ipc("shell", "panel", name, "close")
                    poll(info, lambda value: not value[name]["retained"])
                ipc("shell", "reload")
            after = poll(info, lambda value: value["generation"] != before["generation"])
            assert all(after[k] is None for k in ("calendar", "metrics", "notifications")), after
            disposed = poll(layers, lambda rows: not old_layers.intersection(row["id"] for row in rows))
            assert not any(row["namespace"] == "gnoblin-shell-popup" and row["mapped"] for row in disposed), disposed
            assert ipc("shell", "panel", name, "open")["ok"]
            fresh = poll(info, lambda value: value[name] is not None and value[name]["visible"])
            assert (
                fresh["calendarAnchor"]
                and fresh["metricsBindings"]
                and fresh["notificationBindings"]
                and fresh["notificationLink"]
            ), fresh
            poll(
                layers,
                lambda rows: any(
                    row["mapped"]
                    and row["keyboard_interactive"] == "exclusive"
                    and row["namespace"] in ("gnoblin-shell-popup", "bingux-notifications")
                    for row in rows
                ),
            )
            if name == "calendar":
                poll(
                    layers,
                    lambda rows: any(
                        row["namespace"] == "gnoblin-shell-popup" and row["keyboard_interactive"] == "exclusive"
                        for row in rows
                    ),
                )
                qmp("send-key", keys=[{"type": "qcode", "data": "right"}])
                poll(info, lambda value: value["calendarDate"] != fresh["calendarDate"])
            escape()
            poll(info, lambda value: not value[name]["visible"])
            stage[condition] = {"before": before, "after": after, "reopened": fresh, "layers": layers()}
        records.append(stage)
        (output / "records.json").write_text(json.dumps(records, indent=2))
        # A fresh engine gives the next popup the same cold first-use route.
        previous = info()["generation"]
        ipc("shell", "reload")
        poll(info, lambda value: value["generation"] != previous)
    stop.set()
    worker.join()
    maximum = max(
        sum(row["namespace"] == "gnoblin-shell-popup" and row["mapped"] for row in sample.get("layers", []))
        for sample in samples
    )
    # The metrics popup keeps its overflow host mapped: these are two distinct windows.
    assert maximum <= 2, maximum
    history_maximum = max(
        sum(row["namespace"] == "bingux-notifications" and row["mapped"] for row in sample.get("layers", []))
        for sample in samples
    )
    assert history_maximum <= 1, history_maximum
    (output / "result.json").write_text(
        json.dumps(
            {
                "ok": True,
                "panels": len(records),
                "reload_cases": 9,
                "native_layer_samples": len(samples),
                "max_mapped_popups": maximum,
                "native_popup_disposal_cases": 6,
                "history_binding_reload_cases": 3,
            },
            indent=2,
        )
    )
    print((output / "result.json").read_text())
finally:
    stop.set()
    if worker:
        worker.join()
    (output / "layer-samples.json").write_text(json.dumps(samples, indent=2))
    guest("python3", "-c", write, target, data=original)
    guest("sudo", "-n", "systemctl", "restart", "gdm")
    stream.close()
    sock.close()
