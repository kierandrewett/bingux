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
    property int step: 0
    property int failures: 0
    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            const view = dots.children.find(child => child.contentX !== undefined);
            const item = view.itemAtIndex(dots.activeIndex);
            if (dots.activeIndex >= 0 && (!item || item.x - view.contentX < -0.1 || item.x + item.width - view.contentX > view.width + 0.1)) {
                console.error("PILL_CLIPPED", step, view.originX, view.contentX, item?.x);
                failures++;
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
                console.info(failures ? "INDICATOR_TEST_FAILED" : "INDICATOR_TEST_PASSED");
                Qt.quit();
            }
        }
    }
}
