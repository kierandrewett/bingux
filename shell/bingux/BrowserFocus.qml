import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "BrowserWindow.js" as BrowserWindow

// Portal opening from a background service lacks the input surface's activation
// token. The shell explicitly raises the configured browser, as the dock does.
Item {
    id: root
    property var windows: ToplevelManager.toplevels.values
    property string desktopId: ""
    property string queryTitle: ""
    property int attempts: 0

    function cancel() {
        retry.stop();
        lookup.running = false;
        desktopId = "";
    }
    function request(title) {
        cancel();
        queryTitle = title;
        attempts = 0;
        lookup.running = true;
    }
    Process {
        id: lookup
        command: ["/usr/bin/xdg-mime", "query", "default", "x-scheme-handler/https"]
        stdout: StdioCollector {
            onStreamFinished: {
                root.desktopId = text.trim();
                if (root.desktopId)
                    retry.restart();
            }
        }
    }
    Timer {
        id: retry
        interval: 100
        repeat: true
        onTriggered: {
            root.attempts++;
            const entry = DesktopEntries.byId(root.desktopId) || DesktopEntries.byId(root.desktopId.replace(/\.desktop$/, ""));
            const window = BrowserWindow.choose(root.windows, root.desktopId, entry ? entry.startupClass : "", root.queryTitle, root.attempts >= 5);
            if (window) {
                stop();
                window.activate();
            } else if (root.attempts >= 30) {
                stop();
            }
        }
    }
}
