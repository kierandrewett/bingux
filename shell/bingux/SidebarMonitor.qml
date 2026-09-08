import QtQuick
import QtQuick.Controls
import QtCore
import Quickshell

Item {
    id: root
    property var metrics: null
    property bool ready: false
    Component.onCompleted: ready = true
    function focusContent() { forceActiveFocus(); }
    Settings {
        id: saved
        location: "file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-system.ini"
        property string range: "1m"
    }
    QtObject {
        id: unavailableMetrics
        property bool available: false
        property var latest: null
        property var history: []
        function formatBytes(value) { return "—"; }
        function formatRate(value) { return "—"; }
    }
    // Share the top bar's readout model and graph components, including gaps,
    // historical inspection, and rate scaling. No second metrics poller.
    SystemMetrics {
        id: monitor
        visible: false
        systemMetrics: root.metrics || unavailableMetrics
    }
    SystemPerformance {
        objectName: "sidebarPerformance"
        anchors.fill: parent
        compact: true
        monitorWidget: monitor
        range: saved.range === "5m" ? "5m" : "1m"
        onRangeChanged: {
            if (!root.ready) return;
            saved.range = range;
            saved.setValue("range", range);
            saved.sync();
        }
    }
}
