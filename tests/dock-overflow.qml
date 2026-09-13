import QtQuick
import QtTest
import Quickshell

ShellRoot {
    component TestWindow: QtObject {
        property string appId
        property string title: appId
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        function activate() {
            manager.activeToplevel = this;
        }
        function close() {
        }
        function setRectangle(window, rectangle) {
        }
    }

    TestWindow {
        id: seedWindow
        appId: "seed"
        title: "Seed"
        activated: true
    }

    QtObject {
        id: manager
        property var activeToplevel: seedWindow
        property QtObject toplevels: QtObject {
            property var values: [seedWindow]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    QtObject {
        id: testSettings
        property var pinnedApps: []
    }
    Dock {
        id: dock
        testManager: manager
        settings: testSettings
    }

    TestCase {
        when: true
        function test_running_app_overflow_scroll() {
            tryVerify(() => dock.visible, 5000, "dock becomes visible for the overflow test");
            compare(dock.iconSizeForWidth(1920, 8, 0, 56), 56, "preferred size is retained when the row fits");
            compare(dock.iconSizeForWidth(500, 8, 0, 56), 38, "available width determines the reduced icon size");
            compare(dock.iconSizeForWidth(200, 24, 1, 56), dock.minimumIconSize, "very crowded rows stop at the minimum icon size");
            const compactWindows = [];
            for (let index = 0; index < 8; index++)
                compactWindows.push(TestWindow.createObject(dock, {
                    appId: "compact-" + index,
                    title: "Compact " + index
                }));

            manager.toplevels.values = compactWindows;
            dock.refreshAppGroups();
            tryCompare(dock.testItems, "count", compactWindows.length);

            const windows = [];
            for (let index = 0; index < 24; index++)
                windows.push(TestWindow.createObject(dock, {
                    appId: "running-" + index,
                    title: "Running " + index
                }));

            testSettings.pinnedApps = ["running-0"];
            manager.toplevels.values = windows;
            dock.refreshAppGroups();
            tryCompare(dock.testItems, "count", windows.length);
            tryCompare(dock, "pinnedGroupCount", 1);
            tryVerify(() => dock.iconSize === dock.minimumIconSize, 1000, "crowded dock scales to the minimum on the 1280px test monitor");
            verify(dock.itemSize === dock.iconSize + 16, "dock item geometry follows the scaled icon size");

            const viewport = findChild(dock.contentItem, "dockLiveWidgets");
            verify(viewport.contentWidth > viewport.width, "running apps overflow the dock viewport");
            const runningButton = dock.testItems.itemAt(1);
            const before = viewport.contentX;
            mouseWheel(runningButton.testMouse, 20, 20, 0, -120);
            tryVerify(() => viewport.contentX > before, 500, "wheel over a running app scrolls the dock");
            console.log("DOCK_OVERFLOW_TEST_PASSED");
        }

        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
