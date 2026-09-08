import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    FloatingWindow {
        implicitWidth: 600
        implicitHeight: 400
        HardwareDetails {
            id: panel
            anchors.fill: parent
            monitorWidget: QtObject { property bool available: false; property var sample: null }
        }
        TestCase {
            when: true
            function test_index() {
                wait(100);
                verify(panel.applicationIndex !== null);
                const apps = DesktopEntries.applications.values;
                if (apps.length) compare(panel.applicationFor({name: apps[0].id}), apps[0]);
                compare(panel.applicationFor({}), null);
                report.setText("FAILURES 0\n");
                finish.start();
            }
        }
    }
}
