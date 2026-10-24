import QtQuick
import Quickshell
import Quickshell.Io

// The desktop's right-click menu. It opens terminals and Bingux Settings pages.
// Each action probes its target first so a missing program is reported
// visibly instead of silently doing nothing.
TrayMenu {
    id: root

    property point requestedPosition: Qt.point(0, 0)
    signal requestFailed(string message)
    signal dispatched(string program)

    preferredX: requestedPosition.x
    preferredY: requestedPosition.y
    compactRows: true
    actions: [
        action("Open Terminal", "xdg-terminal-exec", ["xdg-terminal-exec"], "Could not open Terminal", "Install a terminal that provides xdg-terminal-exec."),
        action("Bingux Settings", Python.settings, settingsCommand([]), "Could not open Bingux Settings", "The Bingux Settings program is not installed."),
        action("Display Settings", Python.settings, settingsCommand(["display"]), "Could not open Display Settings", "The Bingux Settings program is not installed."),
        action("Change Background…", Python.settings, settingsCommand(["background"]), "Could not open Change Background", "The Bingux Settings program is not installed.")
    ]

    // Open the menu at a pointer position on a specific monitor. The shell IPC
    // resolves the monitor name; the menu owns the rest of the placement.
    function openAt(target, x, y) {
        if (!target || !Number.isFinite(x) || !Number.isFinite(y))
            return;
        screen = target;
        requestedPosition = Qt.point(x, y);
        visible = true;
    }

    function settingsCommand(page) {
        return [Python.python, Python.applicationSpawn, "--", Python.settings].concat(page);
    }

    function action(text, binary, command, failureTitle, failureDetail) {
        return {
            text,
            enabled: true,
            isSeparator: false,
            hasChildren: false,
            checkState: Qt.Unchecked,
            triggered: () => root.dispatch(binary, command, failureTitle, failureDetail)
        };
    }

    function dispatch(binary, command, failureTitle, failureDetail) {
        probe.createObject(root, {
            binary,
            done: code => {
                if (code === 0) {
                    Quickshell.execDetached(command);
                    root.dispatched(binary);
                    return;
                }
                root.reportFailure(failureTitle, failureDetail);
            }
        });
    }

    function reportFailure(title, detail) {
        console.warn("bingux-desktop-menu: " + title + ": " + detail);
        ShellNotifications.send(title, detail, {
            icon: "dialog-error-symbolic"
        });
        requestFailed(title + ". " + detail);
    }

    // `command -v` exits non-zero when the program is absent or not executable.
    Component {
        id: probe
        Process {
            property string binary: ""
            property var done: null
            command: ["sh", "-c", "command -v \"$1\" >/dev/null 2>&1", "sh", binary]
            running: true
            onExited: code => {
                const callback = done;
                destroy();
                if (callback)
                    callback(code);
            }
        }
    }
}
