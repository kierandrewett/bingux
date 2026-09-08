import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_METRICS_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    Window {
        id: window
        width: 620
        height: 440
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        color: Theme.shellSurface
        ProcessTable {
            id: table
            anchors.fill: parent
            anchors.margins: 16
            records: Array.from({length: 100}, (_, i) => ({pid: i + 1, name: "Process " + i, executable: "", cpuPercent: i, memoryBytes: (100 - i) * 1024, state: i % 2 ? "R" : "S"}))
            totalCount: records.length
            applicationFor: record => null
            formatBytes: value => value + " B"
        }
        TestCase {
            when: window.visible
            property string checks: ""
            function equal(actual, expected, message) {
                checks += message + ": " + actual + " / " + expected + "\n";
                report.setText(checks);
                compare(actual, expected, message);
            }
            function test_shared_table() {
                wait(150);
                const list = findChild(table, "processRows");
                equal(list.count, 100, "Process rows retained");
                equal(table.recordAt(0).pid, 100, "Initial CPU sort retained");
                mouseClick(findChild(table, "processSort_Memory"));
                equal(table.recordAt(0).pid, 1, "Memory sort retained");
                mouseClick(list.itemAtIndex(0), 40, 17);
                mouseClick(list.itemAtIndex(2), 40, 17, Qt.LeftButton, Qt.ShiftModifier);
                equal(table.selectedRecords.length, 3, "Shift selection retained");
                keyClick(Qt.Key_A, Qt.ControlModifier);
                equal(table.selectedRecords.length, 100, "Keyboard select all retained");
                keyClick(Qt.Key_Escape);
                equal(table.selectedRecords.length, 0, "Escape clears selection");
                list.positionViewAtBeginning();
                mouseWheel(list, 80, 60, 0, -120);
                equal(list.contentY - list.originY, 136, "Four process rows per wheel step");
                const offset = list.contentY - list.originY;
                table.records = table.records.map(row => Object.assign({}, row));
                wait(50);
                equal(list.contentY - list.originY, offset, "Refresh preserves scroll");
                window.width = 360;
                wait(50);
                const viewport = findChild(table, "processTableViewport");
                mouseWheel(list, 80, 60, 0, -120, Qt.NoButton, Qt.ShiftModifier);
                equal(viewport.contentX > 0, true, "Horizontal scroll retained");
                viewport.contentX = 0;
                table.mode = "services";
                table.records = [{key: "a", name: "alpha.service", description: "First service", state: "active"}, {key: "b", name: "beta.service", description: "Second service", state: "failed"}];
                wait(50);
                mouseClick(findChild(table, "processSort_State"));
                equal(table.recordAt(0).state, "active", "First service state group");
                mouseClick(findChild(table, "processSort_State"));
                equal(table.recordAt(0).state, "failed", "Cycle service state group");
                report.setText("PASS: shared table retains process sorting, selection, keyboard, wheel, refresh and service state grouping\n");
            }
            function cleanupTestCase() { window.visible = false; finish.start(); }
        }
    }
}
