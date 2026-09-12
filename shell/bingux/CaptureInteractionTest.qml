import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
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
        function test_interactions() {
            report.setText("FAIL: interaction test did not finish\n");
            report.setText("FAIL: menus dismissed before frozen frame=" + !frozenBeforeDismiss + "\n");
            verify(frozenBeforeDismiss, "Freeze menus before requesting their dismissal");
            const surface = capture.previewItem;
            const toolbar = find(surface, "captureToolbar");
            verify(toolbar !== null);
            tryCompare(toolbar, "opacity", 1, 1000);
            compare(toolbar.height, 56);
            verify(capture.previewToken !== "", "Preview is frozen, not live");
            const images = capture.previews[capture.activeScreen.name];
            verify(images.plain !== images.cursor, "Cursor variants are separate frozen images");
            const cursor = find(surface, "captureCursorToggle");
            const before = cursor.chosen;
            mouseClick(cursor);
            compare(cursor.chosen, !before, "Cursor switch changes preview mode");
            mousePress(surface, 30, 80);
            mouseMove(surface, surface.width - 1, surface.height - 1);
            report.setText("FAIL: edge selection " + capture.region + " screen " + surface.width + "x" + surface.height + "\n");
            compare(capture.region.x + capture.region.width, surface.width, "Drawing reaches the last screen column");
            compare(capture.region.y + capture.region.height, surface.height, "Drawing reaches the last screen row");
            mouseRelease(surface, surface.width - 1, surface.height - 1);
            capture.resetRegion();
            const resize = find(surface, "captureRegionResize1_1");
            mousePress(resize, resize.width / 2, resize.height / 2);
            mouseMove(surface, surface.width - 1, surface.height - 1);
            compare(capture.region.x + capture.region.width, surface.width, "Resize reaches right edge");
            compare(capture.region.y + capture.region.height, surface.height, "Resize reaches bottom edge");
            mouseRelease(surface, surface.width - 1, surface.height - 1);
            capture.resetRegion();
            const region = find(surface, "captureRegionMove");
            report.setText("FAIL: move/drag fade\n");
            mousePress(region, region.width / 2, region.height / 2);
            mouseMove(region, region.width / 2 + 20, region.height / 2 + 20);
            tryVerify(() => Math.abs(toolbar.opacity - .25) < .01, 500);
            mouseRelease(region);
            tryCompare(toolbar, "opacity", 1, 500);
            for (const point of [Qt.point(0, 0), Qt.point(surface.width - 1, surface.height - 1)]) {
                mousePress(region, region.width / 2, region.height / 2);
                const width = capture.region.width, height = capture.region.height;
                mouseMove(surface, point.x, point.y);
                verify(capture.regionDragging, "Moving keeps ownership at the screen edge");
                compare(capture.region.x, point.x === 0 ? 0 : surface.width - width);
                compare(capture.region.y, point.y === 0 ? 0 : surface.height - height);
                compare(capture.region.width, width);
                compare(capture.region.height, height);
                mouseRelease(surface, point.x, point.y);
            }
            capture.resetRegion();
            const handle = find(surface, "captureToolbarHandle");
            report.setText("FAIL: toolbar grip\n");
            const oldY = toolbar.y;
            mousePress(handle, 12, 20);
            mouseMove(surface, toolbar.x + 20, toolbar.y - 80);
            mouseRelease(handle);
            verify(toolbar.y < oldY - 40, "Toolbar moves with its handle");
            report.setText("FAIL: settings toggle or menu lookup\n");
            mouseClick(find(surface, "captureSettingsToggle"));
            report.setText("FAIL: settings opened=" + capture.optionsOpen + "\n");
            verify(capture.optionsOpen);
            waitForPolish(surface);
            wait(20);
            const qualityField = find(surface, "captureChoiceQuality");
            mouseClick(qualityField);
            waitForPolish(surface);
            wait(20);
            const qualityMenu = qualityField.parent.popup;
            report.setText("FAIL: dropdown visible=" + qualityMenu.visible + " x=" + qualityMenu.panelX + " field x=" + qualityField.mapToItem(surface, 0, 0).x + " widths=" + qualityMenu.popupWidth + "/" + qualityField.width + "\n");
            verify(qualityMenu.visible);
            compare(qualityMenu.popupWidth, qualityField.width, "Dropdown matches field width");
            compare(qualityMenu.panelX, qualityField.mapToItem(surface, 0, 0).x, "Dropdown aligns after toolbar moves");
            keyClick(Qt.Key_Down);
            keyClick(Qt.Key_Return);
            keyClick(Qt.Key_Escape);
            tryCompare(capture, "opened", false, 500);
            report.setText("PASS: exact drawing/resizing/moving screen edges, frozen cursor preview, drag fade, toolbar drag, custom menu, Escape after focused controls\n");
            finish.start();
        }
    }
}
