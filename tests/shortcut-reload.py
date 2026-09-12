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

    def serve():
        c, _ = sock.accept()
        c.sendall(b'{"event":"hello","features":[]}\n')
        f = c.makefile("r")
        for line in f:
            r = json.loads(line)
            if r["op"] == "bind":
                attempts.append(r)
                response = (
                    {"event": "error", "message": "shortcut already claimed: Super"}
                    if len(attempts) == 1
                    else {"event": "bound", "id": r["id"]}
                )
                c.sendall((json.dumps(response) + "\n").encode())

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
 ShortcutSession { bindings: [{id: "search", accelerator: "Super", hold: 0}]; onReadyChanged: if (ready) { console.log("RETRY_PASS"); Qt.quit(); } }
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
    assert r.returncode == 0 and "RETRY_PASS" in r.stdout + r.stderr and len(attempts) == 2, (
        r.stdout,
        r.stderr,
        attempts,
    )
    print("PASS: real QML transport retries a reload ownership conflict and becomes ready")
