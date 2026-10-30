import QtQuick
import QtTest
import Quickshell

// Dock order, labels and closed-group cleanup, with deterministic window events.
ShellRoot {
    component TestWindow: QtObject {
        property string appId: ""
        property string title: "Window"
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        function close() {
        }
        function activate() {
        }
        function setRectangle(window, rect) {
        }
    }
    TestWindow {
        id: a
        appId: "ord-a"
    }
    TestWindow {
        id: b
        appId: "ord-b"
    }
    TestWindow {
        id: a2
        appId: "ord-a"
    }
    TestWindow {
        id: mystery
        title: "Mystery tool"
    }
    QtObject {
        id: manager
        property var activeToplevel: a
        property QtObject toplevels: QtObject {
            property var values: [a, b]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    QtObject {
        id: config
        property var pinnedApps: []
    }
    Dock {
        id: dock
        settings: config
        testManager: manager
    }
    TestCase {
        when: dock.startupReady
        property bool failed: false
        function check(condition, message) {
            if (!condition && !failed) {
                failed = true;
                console.warn("DOCK_BEHAVIOUR_FAILED " + message);
            }
            return condition;
        }
        function ids() {
            return dock.appGroups.map(group => group.id);
        }
        function open(window) {
            manager.toplevels.values = manager.toplevels.values.concat([window]);
            manager.toplevels.objectInsertedPost(window, manager.toplevels.values.length - 1);
        }
        function close(window) {
            const index = manager.toplevels.values.indexOf(window);
            manager.toplevels.values = manager.toplevels.values.filter(item => item !== window);
            manager.toplevels.objectRemovedPost(window, index);
        }
        function test_order_labels_and_cleanup() {
            tryCompare(dock.testItems, "count", 2);
            wait(300);
            check(JSON.stringify(ids()) === JSON.stringify(["ord-a", "ord-b"]), "start order " + ids());

            // A second window in an existing group moves that group to the end.
            open(a2);
            tryVerify(() => dock.appGroups.length === 2 && dock.appGroups[0].id === "ord-b");
            check(JSON.stringify(ids()) === JSON.stringify(["ord-b", "ord-a"]), "grouped window did not move its group: " + ids());
            check(dock.appGroups[1].windows.length === 2, "group lost a window");

            // A window with no app ID is named, and gets a stable id.
            open(mystery);
            tryVerify(() => dock.appGroups.length === 3);
            const mysteryGroup = dock.appGroups[2];
            check(mysteryGroup.id.indexOf("window-") === 0, "unmatched window id " + mysteryGroup.id);
            check(mysteryGroup.displayName === "Mystery tool", "unmatched window name " + mysteryGroup.displayName);
            check(dock.appGroups[2].id === mysteryGroup.id && ids().indexOf("ord-b") < 2, "order after open " + ids());

            // Closing the last window removes the slot, not just its icon.
            close(mystery);
            tryVerify(() => dock.appGroups.every(group => group.id.indexOf("window-") !== 0), 4000);
            tryCompare(dock.testItems, "count", 2, 4000);
            check(JSON.stringify(ids()) === JSON.stringify(["ord-b", "ord-a"]), "order after close " + ids());

            // Closing one window of a two-window group keeps the group and its place.
            close(a2);
            wait(600);
            check(JSON.stringify(ids()) === JSON.stringify(["ord-b", "ord-a"]), "order after closing one of two " + ids());
            check(dock.appGroups[1].windows.length === 1, "group window count after close");

            // A reopened window of a departed app is a new, last group.
            close(b);
            tryCompare(dock.testItems, "count", 1, 4000);
            open(b);
            tryVerify(() => dock.appGroups.length === 2);
            check(JSON.stringify(ids()) === JSON.stringify(["ord-a", "ord-b"]), "reopened app is last: " + ids());
            if (!failed)
                console.info("DOCK_TEST_PASSED");
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
