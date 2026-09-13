import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }

    FloatingWindow {
        visible: true
        implicitWidth: 500
        implicitHeight: 100
        color: "black"
    }

    SnapAssist {
        id: snap
    }

    TestCase {
        when: true

        function initTestCase() {
            report.setText("RUNNING");
        }

        function cleanupTestCase() {
            report.setText("FAILURES " + qtest_results.failCount);
        }

        property int nextSerial: 0

        function pose(x, y, maximized, newDrag) {
            const screen = {x: 0, y: 0, width: 1280, height: 800};
            snap.update({
                active: true,
                serial: newDrag ? ++nextSerial : nextSerial,
                window: "test-window",
                x,
                y,
                modifiers: 0,
                maximized,
                monitor: {id: 0, x: screen.x, y: screen.y, width: screen.width, height: screen.height},
                area: {x: screen.x, y: screen.y + 32, width: screen.width, height: screen.height - 32}
            });
        }

        function test_maximized_drag_hides_snap_assist() {
            pose(640, 400, false, true);
            compare(snap.dragAssistAllowed, true, "normal titlebar drags allow snap assist");
            pose(650, 400, false, false);
            verify(snap.regions.length > 0, "normal titlebar drags publish snap regions");
            pose(640, 400, true, true);
            compare(snap.dragAssistAllowed, false, "maximized titlebar drags suppress snap assist");
            compare(snap.regions.length, 0, "maximized titlebar drags publish no hidden snap targets");
        }

        function test_picker_requires_entering_pill_area_and_shares_motion() {
            pose(640, snap.pickerY, false, true);
            compare(snap.expanded, false, "a titlebar click inside the pill area does not open the picker");
            compare(snap.pickerProgress, 0, "a titlebar click does not start picker motion");

            pose(640, 8, false, true);
            compare(snap.expanded, false, "near the top edge but outside the pill does not open the picker");
            compare(snap.pickerProgress, 0, "picker stays closed outside the pill area");

            pose(snap.centerX, snap.pickerY, false, false);
            compare(snap.expanded, true, "entering the pill area opens the picker");
            if (!Theme.reducedMotion)
                tryVerify(() => snap.pickerProgress > 0 && snap.pickerProgress < 1, 100);
            tryCompare(snap, "pickerProgress", 1, 1000);

            pose(640, 400, false, false);
            tryCompare(snap, "pickerProgress", 0, 1000);
        }
    }
}
