import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import "../shell/bingux"

ShellRoot {
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
    }
    FileView {
        id: report
        path: Quickshell.env("CAPTURE_ENTER_REPORT")
    }
    Timer {
        interval: 100
        running: true
        onTriggered: capture.open()
    }
    Timer {
        id: finish
        interval: 100
        onTriggered: Qt.quit()
    }
    TestCase {
        name: "CaptureEnter"
        parent: capture.previewItem
        when: capture.opened && capture.previewItem !== null
        function find(item, name) {
            if (item.objectName === name)
                return item;
            for (const child of item.children || []) {
                const found = find(child, name);
                if (found)
                    return found;
            }
            return null;
        }
        function test_enter_from_toolbar() {
            for (const key of [Qt.Key_Return, Qt.Key_Enter]) {
                if (!capture.opened)
                    capture.open();
                tryCompare(capture, "opened", true, 5000);
                const surface = capture.previewItem;
                verify(surface !== null);
                waitForPolish(surface);
                if (key === Qt.Key_Return) {
                    report.setText("FAIL: Enter must select a settings menu item without capturing");
                    mouseClick(find(surface, "captureSettingsToggle"));
                    verify(capture.optionsOpen);
                    const quality = find(surface, "captureChoiceQuality");
                    mouseClick(quality);
                    const menu = quality.parent.popup;
                    tryCompare(menu, "visible", true, 1000);
                    keyClick(Qt.Key_Down);
                    keyClick(Qt.Key_Return);
                    tryCompare(menu, "visible", false, 1000);
                    verify(capture.opened);
                    capture.optionsOpen = false;
                }
                const cursor = find(surface, "captureCursorToggle");
                verify(cursor !== null);
                cursor.forceActiveFocus();
                tryCompare(cursor, "activeFocus", true, 1000);
                report.setText("FAIL: Enter key " + key + " did not save from toolbar focus");
                keyClick(key);
                tryCompare(capture, "state", "saved", 5000);
                verify(!capture.opened);
                verify(capture.savedPath.startsWith(Quickshell.env("CAPTURE_ENTER_OUTPUT") + "/"));
            }
            report.setText("CAPTURE_ENTER_PASSED");
            console.info("CAPTURE_ENTER_PASSED");
            finish.start();
        }
    }
}
