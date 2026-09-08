import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    property int performanceClicks: 0
    property int configureClicks: 0
    QtObject {
        id: sample
        property bool available: true
        property var latest: ({cpuPercent: 24, memoryUsedBytes: 12.4 * 1073741824, memoryTotalBytes: 32 * 1073741824, networkReceiveBytesPerSecond: 240000, networkTransmitBytesPerSecond: 32000, extra: {cpuCores: Array.from({length: 16}, (_, id) => ({id, usage: id * 5})), cpuTemperatureCelsius: 54.2, load1: 2.35, logicalCpus: 16, swapUsedBytes: 1073741824, swapTotalBytes: 8589934592, diskReadBytesPerSecond: 24000000, diskWriteBytesPerSecond: 1200000}})
        property var history: []
        readonly property string cpuLabel: "CPU " + latest.cpuPercent + "%"
        function formatRate(rate) { return Math.round(rate / 1024) + "K/s"; }
        function formatBytes(bytes) { return (bytes / 1073741824).toFixed(1) + "G"; }
    }
    RollingNumber { id: rollProbe; text: "9"; value: 9; visible: false }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: results; path: Quickshell.env("BINGUX_METRICS_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 560
        implicitHeight: 960
        color: Theme.barBackground
        SystemMetrics {
            id: widget
            anchors.horizontalCenter: parent.horizontalCenter
            y: 4
            systemMetrics: sample
            preferencesLocation: Qt.resolvedUrl("monitors.ini")
            onConfigureRequested: { configureClicks++; popup.showPage(true); }
            onPerformanceRequested: { performanceClicks++; popup.showPage(false); }
        }
        SystemMetricsPopup { id: popup; hostItem: window.contentItem; monitorWidget: widget }
        TestCase {
            when: window.visible
            function test_popup_bounds() {
                popup.showPage(false);
                wait(300);
                verify(popup.popupWidth <= 520 && popup.popupHeight <= 640);
                verify(popup.popupHeight <= window.height * .65);
                const scroll = findChild(popup.contentItem, "monitorPageScroll").contentItem;
                compare(scroll.boundsBehavior, Flickable.StopAtBounds);
                compare(scroll.boundsMovement, Flickable.StopAtBounds);
                scroll.contentY = scroll.originY;
                scroll.flick(0, 2000);
                wait(100);
                compare(scroll.contentY, scroll.originY);
                const bottom = Math.max(scroll.originY, scroll.contentHeight - scroll.height);
                scroll.contentY = bottom;
                scroll.flick(0, -2000);
                wait(100);
                compare(scroll.contentY, bottom);
                mouseClick(findChild(popup.contentItem, "performancePage_processes"));
                wait(100);
                verify(popup.popupWidth <= 520 && popup.popupHeight <= 640);
                popup.visible = false;
                tryVerify(() => !popup.retained);
                results.setText("PASS: compact overview and processes; no top or bottom overshoot\n");
                finish.start();
            }
        }
    }
}
