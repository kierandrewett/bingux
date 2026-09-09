import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Metrics { id: liveMetrics }
    FloatingWindow {
        implicitWidth: 230; implicitHeight: 900
        SidebarMonitor { id: sidebar; anchors.fill: parent; metrics: liveMetrics }
        TestCase {
            when: true
            function test_updates() {
                wait(100);
                const performance = findChild(sidebar, "sidebarPerformance");
                performance.page = "processes";
                const panel = findChild(performance, "hardwareDetails");
                tryVerify(() => panel.processRecords.length > 0, 4000);
                const revision = panel.processRevision;
                const table = findChild(panel, "processTable");
                verify(table !== null);
                verify(table.list.count > 0);
                wait(200);
                mouseWheel(table.list, 80, 60, 0, -120);
                compare(table.list.contentY - table.list.originY, 136);
                table.list.positionViewAtBeginning();
                wait(100);
                mouseClick(table.list.itemAtIndex(0), 40, 17);
                compare(table.selectedRecords.length, 1);
                tryVerify(() => panel.processRevision > revision, 10000);
                compare(table.list.count, panel.processRecords.length);
                const row = table.list.itemAtIndex(0);
                verify(row !== null);
                const record = panel.processRecords.find(p => p.pid === row.pid);
                verify(record !== undefined);
                compare(row.cpuPercent, record.cpuPercent ?? -1);
                report.setText("FAILURES 0\nrevision " + revision + " -> " + panel.processRevision + " rows " + table.list.count + "\n");
                finish.start();
            }
        }
    }
}
