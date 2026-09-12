pragma Singleton
import QtQuick
import Quickshell

// A stable snapshot for QML consumers. Quickshell updates its ObjectModel row
// by row; binding to its values directly repeats full-list work during a scan.
Scope {
    id: root
    property var entries: []
    Timer {
        id: refresh
        interval: 100
        running: true
        onTriggered: root.entries = Array.from(DesktopEntries.applications.values)
    }
    Connections {
        target: DesktopEntries
        function onApplicationsChanged() {
            refresh.restart();
        }
    }
}
