//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import "../shell/bingux"

// Hosts StatusBar alone at the top of the output for status-bar-parity.py.
// Metrics is the production Metrics component, fed by the same fake
// metrics-v1 socket the desktop baseline uses, so the CPU/RAM readout draws
// the same values as the desktop shell.
ShellRoot {
    id: root

    // Replaced by status-bar-parity.py with the session context to draw.
    Component.onCompleted: SessionContext.enter("__CONTEXT__")

    Metrics {
        id: metrics
    }

    // Desktop-only items are tagged with objectName "parityMask:<name>" and
    // their on-window rects are logged so the parity check can mask them in
    // restricted contexts, where they are intentionally disabled.
    function collect(item, out) {
        const tagged = item.objectName && item.objectName.indexOf("parityMask:") === 0;
        const unavailable = item.objectName && item.objectName.indexOf("controlStatus_") === 0 && item.available === false;
        if ((tagged || unavailable) && item.width > 0) {
            const p = item.mapToItem(null, 0, 0);
            const name = tagged ? item.objectName.substring(12) : item.objectName;
            out.push(name + " " + Math.round(p.x) + " " + Math.round(p.y) + " " + Math.round(item.width) + " " + Math.round(item.height));
        }
        for (const c of item.children) collect(c, out);
    }
    Timer {
        interval: 1500; running: true; repeat: false
        onTriggered: {
            const out = [];
            collect(window.contentItem, out);
            console.error("PARITYRECTS\n" + out.join("\n") + "\nPARITYRECTS_END");
        }
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
