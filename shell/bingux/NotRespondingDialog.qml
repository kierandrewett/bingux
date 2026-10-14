import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

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

    PanelWindow {
        id: dialog
        visible: root.window !== null
        color: "transparent"
        implicitWidth: 420
        implicitHeight: card.implicitHeight + Theme.padding * 2
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-not-responding"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: parent.width - Theme.padding * 2
            implicitHeight: content.implicitHeight + Theme.padding * 2
            height: implicitHeight
            radius: Theme.shellRadius
            color: Theme.shellSurface
            border.width: 1
            border.color: Theme.outline

            ColumnLayout {
                id: content
                anchors.fill: parent
                anchors.margins: Theme.padding
                spacing: Theme.gap

                Text {
                    Layout.fillWidth: true
                    text: "“" + root.appName + "” is not responding"
                    color: Theme.text
                    font.pixelSize: Theme.fontSize + 2
                    font.bold: true
                    wrapMode: Text.WordWrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Text {
                    Layout.fillWidth: true
                    text: "You can wait for it to respond, or force it to quit. Unsaved work in this application is lost."
                    color: Theme.muted
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.gap
                    spacing: Theme.gap
                    Item {
                        Layout.fillWidth: true
                    }
                    ActionButton {
                        text: "Wait"
                        onClicked: root.wait()
                    }
                    ActionButton {
                        text: "Force Quit"
                        onClicked: root.forceQuit()
                    }
                }
            }
        }
    }
}
