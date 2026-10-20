import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    UiSession {
        sessionName: "capture-interaction-test"
        state: ({visible: capture.opened, surface: "bingux-capture", companions: ["bingux-capture-controls"], companionsAbove: true})
    }
    property bool frozenBeforeDismiss: false
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CAPTURE_TEST_RESULTS")
    }
    Timer {
        id: finish
        interval: 300
        onTriggered: Qt.quit()
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
        onOpening: frozenBeforeDismiss = capture.previewToken !== "" && capture.state === "selecting"
    }
    Timer {
        interval: 100
        running: true
        onTriggered: capture.open()
    }
    Timer {
        interval: 15000
        running: true
        onTriggered: Qt.quit()
    }
    TestCase {
        name: "CaptureInteractions"
        parent: capture.previewItem
        when: capture.opened && capture.previewItem !== null && capture.previewItem.width > 0 && capture.selectionItem !== null
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
        function test_interactions() {
            report.setText("FAIL: interaction test did not finish\n");
            report.setText("FAIL: menus dismissed before frozen frame=" + !frozenBeforeDismiss + "\n");
            verify(frozenBeforeDismiss, "Freeze menus before requesting their dismissal");
            const surface = capture.previewItem;
            const selection = capture.selectionItem;
            const toolbar = find(surface, "captureToolbar");
            verify(toolbar !== null);
            tryCompare(toolbar, "opacity", 1, 1000);
            compare(toolbar.height, 176);
            verify(capture.previewToken !== "", "Preview is frozen, not live");
            const images = capture.previews[capture.activeScreen.name];
            verify(images.plain !== images.cursor, "Cursor variants are separate frozen images");
            const cursor = find(surface, "captureCursorToggle");
            const before = cursor.chosen;
            mouseClick(cursor);
            compare(cursor.chosen, !before, "Cursor switch changes preview mode");
            mousePress(selection, 30, 80);
            mouseMove(selection, surface.width - 1, surface.height - 1);
            report.setText("FAIL: edge selection " + capture.region + " screen " + surface.width + "x" + surface.height + "\n");
            compare(capture.region.x + capture.region.width, surface.width, "Drawing reaches the last screen column");
            compare(capture.region.y + capture.region.height, surface.height, "Drawing reaches the last screen row");
            mouseRelease(selection, surface.width - 1, surface.height - 1);
            capture.resetRegion();
            const resize = find(selection, "captureRegionResize1_1");
            mousePress(resize, resize.width / 2, resize.height / 2);
            mouseMove(selection, surface.width - 1, surface.height - 1);
            compare(capture.region.x + capture.region.width, surface.width, "Resize reaches right edge");
            compare(capture.region.y + capture.region.height, surface.height, "Resize reaches bottom edge");
            mouseRelease(selection, surface.width - 1, surface.height - 1);
            capture.resetRegion();
            const region = find(selection, "captureRegionMove");
            report.setText("FAIL: move/drag fade\n");
            mousePress(region, region.width / 2, region.height / 2);
            mouseMove(region, region.width / 2 + 20, region.height / 2 + 20);
            tryVerify(() => Math.abs(toolbar.opacity - .25) < .01, 500);
            mouseRelease(region);
            tryCompare(toolbar, "opacity", 1, 500);
            for (const point of [Qt.point(0, 0), Qt.point(surface.width - 1, surface.height - 1)]) {
                mousePress(region, region.width / 2, region.height / 2);
                const width = capture.region.width, height = capture.region.height;
                mouseMove(selection, point.x, point.y);
                verify(capture.regionDragging, "Moving keeps ownership at the screen edge");
                compare(capture.region.x, point.x === 0 ? 0 : surface.width - width);
                compare(capture.region.y, point.y === 0 ? 0 : surface.height - height);
                compare(capture.region.width, width);
                compare(capture.region.height, height);
                mouseRelease(selection, point.x, point.y);
            }
            capture.resetRegion();
            const handle = find(surface, "captureToolbarHandle");
            report.setText("FAIL: toolbar grip\n");
            const oldY = toolbar.y;
            mousePress(handle, 12, 20);
            mouseMove(surface, toolbar.x + 20, toolbar.y - 80);
            mouseRelease(handle);
            verify(toolbar.y < oldY - 40, "Toolbar moves with its handle");
            verify(capture.configureOptions(JSON.stringify({kind: "recording", audio: "none"})).ok);
            report.setText("FAIL: settings toggle or menu lookup\n");
            mouseClick(find(surface, "captureSettingsToggle"));
            report.setText("FAIL: settings opened=" + capture.optionsOpen + "\n");
            verify(capture.optionsOpen);
            waitForPolish(surface);
            tryCompare(toolbar, "settingsProgress", 1, 1000);
            const quality = find(surface, "captureChoiceQualityHigh");
            mouseClick(quality);
            compare(capture.optionsSnapshot().quality, "high");
            mouseClick(find(surface, "captureSettingsBack"));
            compare(capture.optionsOpen, false);
            keyClick(Qt.Key_Escape);
            tryCompare(capture, "opened", false, 500);
            report.setText("PASS: exact drawing/resizing/moving screen edges, frozen cursor preview, drag fade, toolbar drag, inline settings, Escape after focused controls\n");
            finish.start();
        }
    }
}
