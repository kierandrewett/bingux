import QtQuick
import Quickshell
import Quickshell.Io

// Accounts, sessions and the remembered choice, read once from bingux-session.
QtObject {
    id: root

    property var users: []
    property var sessions: []
    property string rememberedUser: ""
    property string rememberedSession: ""
    property bool loaded: false
    property int pending: 3
    signal ready

    function finish() {
        pending -= 1;
        if (pending === 0) {
            loaded = true;
            ready();
        }
    }

    function parse(text, key, fallback) {
        try {
            const value = JSON.parse(text)[key];
            return value === undefined || value === null ? fallback : value;
        } catch (_) {
            return fallback;
        }
    }

    property Process usersReader: Process {
        command: ["bingux-session", "users"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                root.users = root.parse(text, "users", []);
                root.finish();
            }
        }
    }

    property Process sessionsReader: Process {
        command: ["bingux-session", "sessions"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                root.sessions = root.parse(text, "sessions", []);
                root.finish();
            }
        }
    }

    property Process recallReader: Process {
        command: ["bingux-session", "recall"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                root.rememberedUser = root.parse(text, "user", "");
                root.rememberedSession = root.parse(text, "session", "");
                root.finish();
            }
        }
    }
}
