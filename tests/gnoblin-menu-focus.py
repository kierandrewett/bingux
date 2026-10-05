import json
import os
import shutil
import socket
import subprocess
import tempfile
import threading
from pathlib import Path


os.chdir(Path(__file__).resolve().parent.parent)
with tempfile.TemporaryDirectory(prefix="gnoblin-menu-focus-") as directory:
    root = Path(directory)
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(root / "ipc"))
    listener.listen()
    requests = []
    server_errors = []

    def serve():
        try:
            connection, _ = listener.accept()
            with connection:
                connection.sendall(
                    json.dumps(
                        {
                            "event": "hello",
                            "api_major": 1,
                            "api_minor": 68,
                            "methods": ["shortcut.bind", "shortcut.unbind", "shortcut.session.end"],
                            "events": [],
                            "capabilities": [],
                        }
                    ).encode()
                    + b"\n"
                )
                for line in connection.makefile("r"):
                    record = json.loads(line)
                    requests.append(record)
                    if record.get("op") == "events":
                        connection.sendall(
                            json.dumps({"event": "subscribed", "events": record["events"]}).encode() + b"\n"
                        )
                        for token in ("menu-move-once", "menu-resize-once"):
                            connection.sendall(
                                json.dumps(
                                    {
                                        "event": "gnoblin.window.menu-requested",
                                        "window_id": "42",
                                        "menu_type": "wm",
                                        "x": 100,
                                        "y": 100,
                                        "menu_context": token,
                                    }
                                ).encode()
                                + b"\n"
                            )
                        for event in (
                            {
                                "event": "gnoblin.shortcut.session.activated",
                                "id": "switcher",
                                "session_id": 7,
                                "first": False,
                            },
                            {
                                "event": "gnoblin.shortcut.session.key",
                                "id": "switcher",
                                "session_id": 7,
                                "keyval": 65363,
                                "modifiers": 0,
                                "phase": "press",
                                "focus_context": "older-press-focus",
                            },
                            {
                                "event": "gnoblin.shortcut.session.key",
                                "id": "switcher",
                                "session_id": 7,
                                "keyval": 65363,
                                "modifiers": 0,
                                "phase": "release",
                                "focus_context": "fresh-release-focus",
                            },
                            {
                                "event": "gnoblin.shortcut.session.ended",
                                "id": "switcher",
                                "session_id": 7,
                                "reason": "released",
                            },
                            {
                                "event": "gnoblin.shortcut.session.activated",
                                "id": "switcher",
                                "session_id": 8,
                                "first": False,
                            },
                            {
                                "event": "gnoblin.shortcut.session.key",
                                "id": "switcher",
                                "session_id": 8,
                                "keyval": 65363,
                                "modifiers": 0,
                                "phase": "press",
                                "focus_context": "stale-previous-focus",
                            },
                            {
                                "event": "gnoblin.shortcut.session.key",
                                "id": "switcher",
                                "session_id": 8,
                                "keyval": 65293,
                                "modifiers": 0,
                                "phase": "press",
                                "focus_context": "fresh-enter-focus",
                            },
                        ):
                            connection.sendall(json.dumps(event).encode() + b"\n")
                    elif record.get("op") == "api":
                        connection.sendall(
                            json.dumps(
                                {
                                    "event": "reply",
                                    "id": record["id"],
                                    "result": {"ok": True},
                                }
                            ).encode()
                            + b"\n"
                        )
                    elif record.get("op") == "ping":
                        connection.sendall(
                            json.dumps(
                                {
                                    "event": "reply",
                                    "id": record["id"],
                                    "result": {"pong": "pong"},
                                }
                            ).encode()
                            + b"\n"
                        )
        except (ConnectionResetError, BrokenPipeError):
            pass
        except Exception as error:
            server_errors.append(repr(error))

    threading.Thread(target=serve, daemon=True).start()
    shutil.copy("shell/bingux/ShortcutSession.qml", root)
    (root / "qmldir").write_text(
        "singleton CompositorEnvironment 1.0 CompositorEnvironment.qml\nShortcutSession 1.0 ShortcutSession.qml\n"
    )
    (root / "CompositorEnvironment.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject { readonly property bool gnoblin: true }\n"
    )
    (root / "shell.qml").write_text(
        """import QtQuick
import Quickshell
ShellRoot {
 ShortcutSession {
  id: compositor
  trackWindowMenu: true
  property int menuCount: 0
  property string keyContext: ""
  property bool sawRelease: false
  property bool enterHandled: false
  property int actionsSent: 0
  onWindowMenuRequested: function (request) {
   menuCount++;
   const method = menuCount === 1 ? "window.begin_move" : "window.begin_resize";
   const arguments_ = menuCount === 1 ? {menu_context: request.menu_context} : {menu_context: request.menu_context, edge: "north_west"};
   if (compositor.requestApi(method, arguments_, function (result, error) {}))
    actionsSent++;
  }
  onKeyPressed: function (key, modifiers, context) {
   keyContext = context || "";
   if (key === 65293 || key === 65421) {
    enterHandled = true;
    activateWindow("42", keyContext);
   }
  }
  onKeyReleased: function (key, modifiers, context) { keyContext = context || ""; }
  onReleased: { sawRelease = true; activateWindow("42", keyContext); }
 }
 Timer {
  interval: 20
  repeat: true
  running: true
  onTriggered: if (compositor.menuCount === 2 && compositor.actionsSent === 2 && compositor.sawRelease && compositor.enterHandled) {
   compositor.send({op: "ping", id: "finished"});
   Qt.quit();
  }
 }
 Timer { interval: 5000; running: true; onTriggered: Qt.exit(1) }
}
"""
    )
    result = subprocess.run(
        [os.environ.get("GNOBLIN_QS", "qs"), "-p", str(root / "shell.qml")],
        env={
            **os.environ,
            "QT_QPA_PLATFORM": "offscreen",
            "GNOBLIN_COMPOSITOR_SOCKET": str(root / "ipc"),
        },
        capture_output=True,
        text=True,
        timeout=10,
    )
    api_calls = [record for record in requests if record.get("op") == "api"]
    move = next((record for record in api_calls if record.get("method") == "window.begin_move"), {})
    resize = next((record for record in api_calls if record.get("method") == "window.begin_resize"), {})
    focuses = [record for record in api_calls if record.get("method") == "window.focus"]
    events = next((record.get("events", []) for record in requests if record.get("op") == "events"), [])
    switcher_source = Path("shell/bingux/WindowSwitcher.qml").read_text()
    key_handler = switcher_source.split("function handleShortcutKey(key, modifiers, context) {", 1)[1].split(
        "\n    function finish()", 1
    )[0]
    assert (
        result.returncode == 0
        and "gnoblin.window.menu-requested" in events
        and move.get("arguments") == {"menu_context": "menu-move-once"}
        and resize.get("arguments") == {"menu_context": "menu-resize-once", "edge": "north_west"}
        and [call.get("arguments", {}).get("focus_context") for call in focuses]
        == ["fresh-release-focus", "fresh-enter-focus"]
        and key_handler.index('focusContext = context || "";') < key_handler.index("key === 65293")
        and any(record.get("id") == "finished" for record in requests)
        and not server_errors
    ), (result.stdout, result.stderr, requests, server_errors)
    print("PASS: native menu actions and release-time modal focus refresh use the persistent compositor connection")
