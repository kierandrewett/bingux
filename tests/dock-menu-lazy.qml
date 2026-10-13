import QtQuick
import QtTest
import Quickshell

ShellRoot {
    FloatingWindow {
        implicitWidth: 1
        implicitHeight: 1
    }
    QtObject {
        id: manager
        property var activeToplevel: null
        property QtObject toplevels: QtObject {
            property var values: []
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    component Window: QtObject {
        property string appId
        property string title: appId
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        function setRectangle(window, rect) {
        }
        function close() {
        }
        function activate() {
            activated = true;
            manager.activeToplevel = this;
        }
    }
    Component {
        id: windowFactory
        Window {}
    }
    Dock {
        id: dock
        testManager: manager
        settings: ({
                pinnedApps: []
            })
    }
    Component.onCompleted: {
        const windows = [];
        for (let i = 0; i < 18; i++)
            windows.push(windowFactory.createObject(manager, {
                appId: "lazy-menu-" + i
            }));
        manager.toplevels.values = windows;
        manager.toplevels.objectInsertedPost(windows[0], 0);
    }
    TestCase {
        parent: dock.contentItem
        when: dock.visible
        property bool passed: false
        property string stage: "cold"
        function check(value, message) {
            if (!value)
                console.error("DOCK_BEHAVIOUR_FAILED", message);
            verify(value, message);
        }
        function test_first_use() {
            tryCompare(dock.testItems, "count", 18);
            for (let i = 0; i < 18; i++)
                check(dock.testItems.itemAt(i).testMenu === null, "Cold dock has no hidden app menus");
            stage = "hover";
            const button = dock.testItems.itemAt(0);
            mouseMove(button, button.width / 2, button.height / 2);
            wait(250);
            check(button.testMenu === null, "Tooltip hover does not allocate an app menu");
            stage = "right-click";
            mouseClick(button, button.width / 2, button.height / 2, Qt.RightButton);
            tryVerify(() => button.testMenu !== null && button.menuOpen, 2000);
            const menu = button.testMenu;
            tryVerify(() => menu.visible && menu.presentationOpacity === 1, 2000);
            const action = findChild(menu.body, "dockPinAction");
            check(action !== null, "First click loads real menu actions");
            for (let i = 1; i < 18; i++)
                check(dock.testItems.itemAt(i).testMenu === null, "Unused menus remain absent");
            stage = "outside-dismiss";
            parent = menu.contentItem;
            mousePress(parent, 2, 2, Qt.LeftButton);
            tryVerify(() => !button.menuOpen && !menu.visible, 1000);
            mouseRelease(parent, 2, 2, Qt.LeftButton);
            tryVerify(() => !menu.retained, 1000);
            stage = "reopen-escape";
            button.menuOpen = true;
            tryVerify(() => menu.visible, 1000);
            compare(button.testMenu, menu, "Reopening reuses menu");
            parent = menu.contentItem;
            keyClick(Qt.Key_Escape);
            tryVerify(() => !button.menuOpen, 1000);
            passed = true;
            console.warn("DOCK_TEST_PASSED");
        }
        function cleanupTestCase() {
            if (!passed)
                console.error("DOCK_BEHAVIOUR_FAILED", "Incomplete at", stage);
            Qt.quit();
        }
    }
}
