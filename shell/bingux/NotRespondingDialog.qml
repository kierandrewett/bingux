import QtQuick
import Quickshell
import Quickshell.Io

// Offers Force Quit for a window whose client stopped answering liveness pings. Gnoblin reports
// the state as the window's "unresponsive" field and draws no dialog of its own.
Scope {
    id: root

    // Windows currently unresponsive, by ID, and the ones the user chose to keep waiting for.
    property var unresponsive: ({})
    property var waiting: ({})
    property string windowId: ""
    readonly property var window: windowId ? unresponsive[windowId] ?? null : null
    readonly property string appName: window ? (window.title || window.app_id || "This application") : ""

    function pickNext() {
        const ids = Object.keys(unresponsive).filter(id => !waiting[id]);
        windowId = ids.length ? ids[0] : "";
    }

    function updateWindow(window) {
        const id = String(window.id);
        const next = Object.assign({}, unresponsive);
        const keep = Object.assign({}, waiting);
        if (window.unresponsive === true) {
            next[id] = window;
        } else {
            delete next[id];
            delete keep[id];
        }
        unresponsive = next;
        waiting = keep;
        pickNext();
    }

    function removeWindow(id) {
        updateWindow({
            id,
            unresponsive: false
        });
    }

    function wait() {
        const keep = Object.assign({}, waiting);
        keep[windowId] = true;
        waiting = keep;
        pickNext();
    }

    function forceQuit() {
        killer.windowId = windowId;
        killer.running = true;
    }

    ShortcutSession {
        enabled: true
        trackWindowResponsiveness: true
        onWindowResponsivenessChanged: window => root.updateWindow(window)
        onWindowClosed: id => root.removeWindow(id)
    }

    Process {
        id: killer
        property string windowId: ""
        command: ["gnoblinctl", "window", "kill", windowId]
        onExited: code => {
            if (code === 0)
                root.removeWindow(windowId);
            else
                console.warn("bingux-not-responding: gnoblinctl window kill exited with " + code);
        }
    }

    ShellDialog {
        namespace: "bingux-not-responding"
        open: root.window !== null
        tone: "danger"
        title: "“" + root.appName + "” is not responding"
        body: "You can wait for it to respond, or force it to quit. Unsaved work in this application is lost."
        secondaryText: "Wait"
        primaryText: "Force Quit"
        onSecondaryTriggered: root.wait()
        onPrimaryTriggered: root.forceQuit()
        onDismissed: root.wait()
    }
}
