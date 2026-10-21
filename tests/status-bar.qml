//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import "../shell/bingux"

// Hosts StatusBar alone at the top of the output for status-bar-parity.py.
// Metrics are unavailable, so the CPU/RAM readout draws its placeholder dashes
// the same way the desktop does before bingux-statusd reports.
ShellRoot {
    id: root

    QtObject {
        id: metrics
        property bool desktopStateAvailable: false
        property var inputSources: []
        property bool available: false
        property var sampleSnapshot: null
        property var latest: null
        property var history: []
    }

    PanelWindow {
        id: window
        color: "transparent"
        implicitHeight: Theme.barHeight
        WlrLayershell.namespace: "bingux-status-bar-test"
        anchors {
            top: true
            left: true
            right: true
        }
        StatusBar {
            anchors.fill: parent
            metrics: metrics
            hostWindow: window
        }
    }
}
