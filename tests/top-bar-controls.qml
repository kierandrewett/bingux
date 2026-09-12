import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    property int presses: 0
    FileView {
        id: report
        path: Quickshell.env("BINGUX_TOP_BAR_TEST_RESULTS")
    }
    Timer {
        id: finish
        interval: 300
        onTriggered: Qt.quit()
    }
    FloatingWindow {
        id: window
        implicitWidth: 400
        implicitHeight: 80
        color: Theme.barBackground
        BarSearchButton {
            id: search
            onClicked: presses++
        }
        Pill {
            id: controls
            horizontalPadding: Theme.barPrimaryPadding
            x: 140
            interactive: true
            NotificationIndicator {
                id: notifications
            }
        }
        TestCase {
            when: window.visible
            name: "TopBarControls"
            function test_controls() {
                report.setText("FAIL: top-bar controls did not finish\n");
                waitForRendering(search);
                for (const p of [Qt.point(0, 0), Qt.point(search.width - 1, 0), Qt.point(0, search.height - 1), Qt.point(search.width - 1, search.height - 1)]) {
                    const before = presses;
                    mouseMove(search, p.x, p.y);
                    verify(search.hovered, "Entire search target hovers");
                    mouseClick(search, p.x, p.y);
                    compare(presses, before + 1, "Every search corner activates exactly once");
                }
                const before = presses;
                search.forceActiveFocus();
                keyClick(Qt.Key_Return);
                keyClick(Qt.Key_Space);
                compare(presses, before + 2, "Keyboard activation");
                const badge = findChild(notifications, "notificationCountBadge");
                const count = findChild(notifications, "notificationIndicatorCount");
                verify(!badge.visible);
                notifications.count = 3;
                verify(badge.visible);
                tryCompare(count, "displayedValue", 3, 1000);
                notifications.count = 12;
                tryCompare(count, "displayedValue", 12, 1000);
                compare(controls.horizontalPadding, 16);
                compare(Theme.barIconTarget, 32);
                notifications.count = 0;
                verify(!badge.visible);
                report.setText("PASS: search corner hitboxes and keyboard; animated badge count and zero state; top-bar padding\n");
                finish.start();
            }
        }
    }
}
