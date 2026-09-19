import QtQuick
import Quickshell

ShellRoot {
    QtObject {
        id: a
        property bool activated: false
    }
    QtObject {
        id: b
        property bool activated: true
    }
    QtObject {
        id: c
        property bool activated: false
    }
    QtObject {
        id: d
        property bool activated: false
    }
    QtObject {
        id: e
        property bool activated: false
    }
    QtObject {
        id: f
        property bool activated: false
    }
    FloatingWindow {
        visible: true
        implicitWidth: 100
        implicitHeight: 40
        DockWindowIndicators {
            id: dots
            anchors.centerIn: parent
            windows: [a, b]
        }
    }
    function indicatorView(item) {
        if (item.contentX !== undefined)
            return item;
        for (const child of item.children || []) {
            const found = indicatorView(child);
            if (found)
                return found;
        }
        return null;
    }
    property int step: 0
    property int failures: 0
    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            const view = indicatorView(dots);
            const offset = (view.parent === dots ? 0 : view.x) - view.contentX;
            const viewportWidth = view.parent === dots ? view.width : view.parent.width;
            const item = view.itemAtIndex(dots.activeIndex);
            if (dots.activeIndex >= 0 && (!item || item.x + offset < -0.1 || item.x + item.width + offset > viewportWidth + 0.1)) {
                console.error("PILL_CLIPPED", step, view.originX, view.contentX, item?.x);
                failures++;
            }
            if (step >= 7) {
                const first = view.itemAtIndex(dots.firstVisibleIndex);
                const last = view.itemAtIndex(dots.firstVisibleIndex + dots.visibleCount - 1);
                if (!first || !last || Math.abs(first.x + offset) > .1 || Math.abs(last.x + last.width + offset - viewportWidth) > .1) {
                    console.error("PILL_CLIPPED", "focus loss range", step, dots.activeIndex, dots.firstVisibleIndex, view.originX, view.contentX, view.width, first?.x, last?.x, last?.width);
                    failures++;
                }
            }
            switch (step++) {
            case 0:
                dots.windows = [b];
                break;
            case 1:
                dots.windows = [a, b];
                a.activated = true;
                b.activated = false;
                break;
            case 2:
                a.activated = false;
                b.activated = true;
                break;
            case 3:
                dots.windows = [b];
                break;
            case 4:
                b.activated = false;
                dots.windows = [a, b];
                break;
            case 5:
                dots.windows = [b];
                b.activated = true;
                break;
            case 6:
                b.activated = false;
                dots.windows = [a, b, c, d, e, f];
                f.activated = true;
                break;
            case 7:
                f.activated = false; // Click another application; no window was removed.
                break;
            case 8:
                f.activated = true;
                break;
            case 9:
                f.activated = false;
                c.activated = true;
                break;
            case 10:
                c.activated = false;
                break;
            case 11:
                c.activated = true;
                break;
            case 12:
                console.info(failures ? "INDICATOR_TEST_FAILED" : "INDICATOR_TEST_PASSED");
                Qt.quit();
            }
        }
    }
}
