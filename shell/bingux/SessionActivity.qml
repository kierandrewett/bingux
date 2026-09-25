import QtQuick
import Quickshell
import Quickshell.Io

// Observe session-wide input activity without requiring the shell to own
// keyboard focus. Mutter exposes its idle monitor over the user D-Bus.
Scope {
    id: root

    property bool enabled: true
    property int idleThresholdMs: 2 * 60 * 1000
    property bool available: false
    property bool isAfk: false

    Component.onCompleted: if (enabled)
        monitor.running = true
    onEnabledChanged: {
        if (enabled) {
            if (!monitor.running)
                monitor.running = true;
        } else {
            reconnect.stop();
            monitor.running = false;
            available = false;
            isAfk = false;
        }
    }

    Process {
        id: monitor
        command: [
            "python3",
            "-u",
            decodeURIComponent(Qt.resolvedUrl("session-activity.py").toString().replace(/^file:\/\//, "")),
            String(root.idleThresholdMs)
        ]
        stdout: SplitParser {
            onRead: data => {
                try {
                    const status = JSON.parse(data);
                    if (typeof status.afk === "boolean") {
                        root.available = true;
                        root.isAfk = status.afk;
                    }
                } catch (error) {
                    console.warn("Could not read session activity:", error);
                }
            }
        }
        stderr: SplitParser {
            onRead: data => {
                if (data.trim())
                    console.warn("Session activity monitor:", data.trim());
            }
        }
        onExited: if (root.enabled)
            reconnect.restart()
    }

    Timer {
        id: reconnect
        interval: 30000
        repeat: true
        running: root.enabled && !monitor.running
        onTriggered: if (!monitor.running)
            monitor.running = true
    }
}
