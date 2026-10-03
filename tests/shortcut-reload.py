import os
import socket
import threading
import tempfile
import subprocess
from pathlib import Path
import json
import shutil

os.chdir(Path(__file__).resolve().parent.parent)
with tempfile.TemporaryDirectory(prefix="shortcut-retry-") as d:
    p = Path(d)
    sock = socket.socket(socket.AF_UNIX)
    sock.bind(str(p / "ipc"))
    sock.listen()
    attempts = []
    requests = []

    def serve():
        c, _ = sock.accept()
        c.sendall(b'{"event":"hello","features":[]}\n')
        f = c.makefile("r")
        try:
            for line in f:
                r = json.loads(line)
                requests.append(r)
                if r["op"] == "bind":
                    attempts.append(r)
                    response = (
                        {"event": "error", "message": "shortcut already claimed: Super"}
                        if len(attempts) == 1
                        else {"event": "bound", "id": r["id"]}
                    )
                    c.sendall((json.dumps(response) + "\n").encode())
                elif r["op"] == "ping":
                    c.sendall(
                        (json.dumps({"event": "reply", "id": r["id"], "result": {"pong": "pong"}}) + "\n").encode()
                    )
        except ConnectionResetError:
            pass

    threading.Thread(target=serve, daemon=True).start()
    shutil.copy("shell/bingux/ShortcutSession.qml", p)
    (p / "qmldir").write_text(
        "singleton CompositorEnvironment 1.0 CompositorEnvironment.qml\nShortcutSession 1.0 ShortcutSession.qml\n"
    )
    (p / "CompositorEnvironment.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject { readonly property bool gnoblin: true }\n"
    )
    (p / "shell.qml").write_text("""import QtQuick
import Quickshell
ShellRoot {
 ShortcutSession { bindings: [{id: "search", accelerator: "Super", hold: 0}]; onReadyChanged: if (ready) { send({op: "ping", id: "legacy-ready"}); Qt.quit(); } }
 Timer { interval: 6000; running: true; onTriggered: Qt.exit(1) }
}
""")
    r = subprocess.run(
        [os.environ.get("GNOBLIN_QS", "qs"), "-p", str(p / "shell.qml")],
        env={**os.environ, "QT_QPA_PLATFORM": "offscreen", "GNOBLIN_COMPOSITOR_SOCKET": str(p / "ipc")},
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert r.returncode == 0 and len(attempts) == 2 and any(item.get("id") == "legacy-ready" for item in requests), (
        r.returncode,
        r.stdout,
        r.stderr,
        attempts,
    )
    print("PASS: real QML transport retries a reload ownership conflict and becomes ready")

with tempfile.TemporaryDirectory(prefix="shortcut-native-end-") as d:
    p = Path(d)
    sock = socket.socket(socket.AF_UNIX)
    sock.bind(str(p / "ipc"))
    sock.listen()
    requests = []
    server_errors = []

    def serve_native():
        try:
            c, _ = sock.accept()
            with c:
                c.sendall(
                    json.dumps(
                        {
                            "event": "hello",
                            "version": 1,
                            "api_major": 1,
                            "api_minor": 63,
                            "methods": ["shortcut.bind", "shortcut.unbind"],
                            "events": [],
                            "capabilities": [],
                        }
                    ).encode()
                    + b"\n"
                )
                for line in c.makefile("r"):
                    record = json.loads(line)
                    requests.append(record)
                    if record["op"] == "events":
                        c.sendall(json.dumps({"event": "subscribed", "events": record["events"]}).encode() + b"\n")
                    elif record["op"] == "ping":
                        c.sendall(
                            json.dumps({"event": "reply", "id": record["id"], "result": {"pong": "pong"}}).encode()
                            + b"\n"
                        )
                    elif record["op"] == "api":
                        method = record["method"]
                        if method == "shortcut.bind":
                            c.sendall(
                                json.dumps(
                                    {
                                        "event": "reply",
                                        "id": record["id"],
                                        "result": record["arguments"],
                                    }
                                ).encode()
                                + b"\n"
                            )
                            if sum(item.get("method") == "shortcut.bind" for item in requests) == 1:
                                c.sendall(
                                    json.dumps(
                                        {
                                            "event": "gnoblin.shortcut.binding-activated",
                                            "id": "switcher",
                                            "first": True,
                                            "modifiers": 8,
                                            "session_id": 7,
                                        }
                                    ).encode()
                                    + b"\n"
                                )
                        elif method == "shortcut.unbind":
                            c.sendall(
                                json.dumps(
                                    {
                                        "event": "gnoblin.shortcut.session.ended",
                                        "id": "switcher",
                                        "session_id": 7,
                                        "reason": "unbound",
                                    }
                                ).encode()
                                + b"\n"
                            )
                            c.sendall(
                                json.dumps(
                                    {
                                        "event": "reply",
                                        "id": record["id"],
                                        "result": {"unbound": True},
                                    }
                                ).encode()
                                + b"\n"
                            )
        except ConnectionResetError:
            pass
        except Exception as error:
            server_errors.append(repr(error))

    threading.Thread(target=serve_native, daemon=True).start()
    shutil.copy("shell/bingux/ShortcutSession.qml", p)
    (p / "qmldir").write_text(
        "singleton CompositorEnvironment 1.0 CompositorEnvironment.qml\nShortcutSession 1.0 ShortcutSession.qml\n"
    )
    (p / "CompositorEnvironment.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject { readonly property bool gnoblin: true }\n"
    )
    (p / "shell.qml").write_text("""import QtQuick
import Quickshell
ShellRoot {
 ShortcutSession {
  id: shortcuts
  property bool sawActivation: false
  bindings: [
   {id: "switcher", accelerator: "<Alt>Tab", hold: 8},
   {id: "other", accelerator: "<Super>F1", hold: 67108864}
  ]
  onActivated: function (id, first) { if (first) { sawActivation = true; end(); } }
 }
 Timer { interval: 20; repeat: true; running: true; onTriggered: if (shortcuts.sawActivation && shortcuts.ownedBindings.length === 2 && !shortcuts.bindingRequestPending && !shortcuts.activeSessionId) { shortcuts.send({op: "ping", id: "ready-check"}); Qt.quit(); } }
 Timer { interval: 6000; running: true; onTriggered: Qt.exit(1) }
}
""")
    r = subprocess.run(
        [os.environ.get("GNOBLIN_QS", "qs"), "-p", str(p / "shell.qml")],
        env={
            **os.environ,
            "QT_QPA_PLATFORM": "offscreen",
            "GNOBLIN_COMPOSITOR_SOCKET": str(p / "ipc"),
        },
        capture_output=True,
        text=True,
        timeout=10,
    )
    native_methods = [item.get("method") for item in requests if item.get("op") == "api"]
    native_bind = next(
        (item["arguments"] for item in requests if item.get("method") == "shortcut.bind"),
        {},
    )
    native_events = next(
        (item["events"] for item in requests if item.get("op") == "events"),
        [],
    )
    assert (
        r.returncode == 0
        and native_methods == ["shortcut.bind", "shortcut.bind", "shortcut.unbind", "shortcut.bind"]
        and native_bind["hold"] == "alt"
        and "gnoblin.shortcut.session.ended" in native_events
        and any(item.get("id") == "ready-check" for item in requests)
        and not server_errors
    ), (r.stdout, r.stderr, requests, server_errors)
    print("PASS: native shortcut end unbinds, suppresses cancellation, and rebinds")
