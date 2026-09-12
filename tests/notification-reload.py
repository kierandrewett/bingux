#!/usr/bin/env python3
"""Exercise real notification retention on an isolated D-Bus session."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time


def check_session(directory):
    directory = Path(directory)
    log = (directory / "runtime.log").open("w")
    process = subprocess.Popen(
        [os.environ.get("QS_TEST_BIN", "quickshell"), "-p", str(directory), "--no-color"], stdout=log, stderr=log
    )

    def ipc(method):
        return subprocess.check_output(
            [os.environ.get("QS_TEST_BIN", "quickshell"), "ipc", "-p", str(directory), "call", "test", method],
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()

    def snapshot():
        return json.loads(ipc("snapshot"))

    def await_value(predicate):
        until = time.monotonic() + 8
        while time.monotonic() < until:
            try:
                value = snapshot()
                if predicate(value):
                    return value
            except (subprocess.CalledProcessError, json.JSONDecodeError):
                pass
            time.sleep(0.1)
        raise AssertionError("Notification state did not reach the expected value: " + repr(snapshot()))

    def notify(title, replace=0, timeout=0):
        result = subprocess.check_output(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.Notifications",
                "--object-path",
                "/org/freedesktop/Notifications",
                "--method",
                "org.freedesktop.Notifications.Notify",
                "Files",
                str(replace),
                "system-file-manager",
                title,
                "Reload test",
                "['default', 'Open']",
                "{}",
                str(timeout),
            ],
            text=True,
        )
        return int(re.search(r"uint32 (\d+)", result)[1])

    try:
        await_value(lambda value: value == [])
        ids = [notify(f"Notification {index}", timeout=30000 if index == 1 else 0) for index in range(4)]
        before = await_value(lambda value: len(value) == 4)
        await_value(lambda value: any(not entry["toastVisible"] for entry in value))
        ipc("archiveAll")
        before = await_value(lambda value: len(value) == 4 and all(not entry["toastVisible"] for entry in value))
        ipc("reload")
        after = await_value(lambda value: len(value) == 4 and all(entry["restored"] for entry in value))
        assert all(not entry["toastVisible"] for entry in after)
        for records in (before, after):
            assert sorted(entry["id"] for entry in records) == sorted(ids)
            assert all(entry["actions"] == ["default"] for entry in records)
        assert sorted((entry["id"], entry["summary"], entry["receivedAt"]) for entry in before) == sorted(
            (entry["id"], entry["summary"], entry["receivedAt"]) for entry in after
        )
        notify("Replaced after reload", replace=ids[0])
        await_value(
            lambda value: len(value) == 4 and any(entry["summary"] == "Replaced after reload" for entry in value)
        )
        removed = int(ipc("dismissFirst"))
        await_value(lambda value: len(value) == 3)
        ipc("reload")
        final = await_value(lambda value: len(value) == 3 and all(entry["restored"] for entry in value))
        assert removed not in [entry["id"] for entry in final]
        process.terminate()
        process.wait(timeout=5)
        process = subprocess.Popen(
            [os.environ.get("QS_TEST_BIN", "quickshell"), "-p", str(directory), "--no-color"], stdout=log, stderr=log
        )
        restarted = await_value(lambda value: len(value) == 3)
        assert sorted(entry["summary"] for entry in restarted) == sorted(entry["summary"] for entry in final)
        assert all(not entry["toastVisible"] and entry["actions"] == [] for entry in restarted)
        notify("New notification after restart")
        await_value(lambda value: len(value) == 4)
        ipc("dismissFirst")
        await_value(lambda value: len(value) == 3)
        print(
            "PASS: reload retains live actions; full process restart restores history without replay, and new notifications do not overwrite it"
        )
    except Exception:
        log.flush()
        print((directory / "runtime.log").read_text(), file=sys.stderr)
        raise
    finally:
        process.terminate()
        process.wait(timeout=5)
        log.close()


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--session":
        check_session(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory(prefix="bingux-notification-reload-") as temporary:
            directory = Path(temporary)
            source = Path(__file__).resolve().parent.parent / "shell/bingux/NotificationState.qml"
            shutil.copy(source, directory)
            shutil.copy(source.with_name("NotificationHistory.js"), directory)
            (directory / "qmldir").write_text("NotificationState 1.0 NotificationState.qml\n")
            (directory / "shell.qml").write_text("""import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    NotificationState { id: notifications }
    IpcHandler {
        target: "test"
        function snapshot(): string {
            return JSON.stringify(notifications.allEntries.map(entry => ({
                toastVisible: entry.toastVisible, id: entry.notification.id, summary: entry.summary, receivedAt: entry.receivedAt,
                actions: entry.actions.map(action => action.action.identifier), restored: entry.notification.lastGeneration
            })));
        }
        function archiveAll(): void { notifications.archiveToasts(); }
        function reload(): void { Qt.callLater(() => Quickshell.reload(false)); }
        function dismissFirst(): int {
            const notification = notifications.allEntries[0].notification;
            const id = notification.id;
            notifications.dismiss(notification);
            return id;
        }
    }
}
""")
            subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--session", temporary], check=True)
