import QtQuick
import Quickshell

ShellRoot {
    id: test
    property int step: 0
    property var savedItems: []
    property var failures: []

    function check(condition, message) {
        if (!condition) {
            failures.push(message);
            console.error("FAIL: " + message);
        }
    }
    function identities() {
        const result = [];
        for (let i = 0; i < dock.testItems.count; i++)
            result.push(dock.testItems.itemAt(i));
        return result;
    }
    function sameItems(message) {
        const current = identities();
        check(current.length === savedItems.length && current.every((item, i) => item === savedItems[i]), message);
    }
    function findLabel(item, label) {
        if (item.label === label)
            return item;
        for (const child of item.children || []) {
            const found = findLabel(child, label);
            if (found) return found;
        }
        return null;
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
    component TestWindow: QtObject {
        property string appId: ""
        property string title: ""
        property var parent: null
        property var screens: [] // Valid Gnoblin state: no optional output events.
        property bool activated: false
        property bool minimized: false
        function activate() {
            for (const window of manager.toplevels.values)
                window.activated = window === this;
            manager.activeToplevel = this;
        }
        function setRectangle(window, rect) {}
    }
    TestWindow { id: a; appId: "dock-test-a"; title: "A"; activated: true }
    TestWindow { id: b; appId: "dock-test-b"; title: "B" }
    TestWindow { id: a2; appId: "dock-test-a"; title: "A second window" }
    TestWindow { id: c; appId: "dock-test-c"; title: "C" }

    Dock {
        id: dock
        testManager: manager
        settings: QtObject { property var pinnedApps: [] }
    }
    Timer {
        interval: 300
        running: true
        repeat: true
        onTriggered: {
            switch (test.step++) {
            case 0:
                test.check(dock.appGroups.length === 2, "unfocused zero-output windows stay in dock");
                test.savedItems = test.identities();
                b.activate();
                a.title = "A changed on focus";
                break;
            case 1:
                test.check(dock.appGroups.length === 2, "focus change preserves both apps");
                test.sameItems("title/focus change preserves existing button identities");
                test.check(dock.testItems.itemAt(1).active && !dock.testItems.itemAt(0).active, "active highlight follows focus");
                dock.testItems.itemAt(0).menuOpen = true;
                a.title = "A changed with menu open";
                break;
            case 2:
                test.check(dock.testItems.itemAt(0).menuOpen, "title update preserves open menu");
                const menu = dock.testItems.itemAt(0).testMenu;
                test.check(menu.revealOriginX === menu.popupWidth / 2 && menu.revealOriginY === menu.popupHeight, "menu opens from bottom centre");
                const entry = test.findLabel(menu.body, a.title);
                test.check(entry && entry.iconSource.toString().length > 0, "open-window entry has an app icon");
                dock.testItems.itemAt(0).menuOpen = false;
                manager.toplevels.values = [a, b, a2];
                manager.toplevels.objectInsertedPost(a2, 2);
                break;
            case 3:
                test.sameItems("second window preserves both buttons");
                test.check(dock.appGroups[0].windows.length === 2, "second window updates group membership");
                test.check(dock.testItems.itemAt(0).modelData.windows.length === 2, "rendered button receives changed window count");
                manager.toplevels.values = [a, b, a2, c];
                manager.toplevels.objectInsertedPost(c, 3);
                break;
            case 4:
                test.check(dock.testItems.itemAt(0) === test.savedItems[0] && dock.testItems.itemAt(1) === test.savedItems[1], "new app does not recreate existing buttons");
                manager.toplevels.values = [a, b];
                manager.toplevels.objectRemovedPost(a2, 2);
                manager.toplevels.objectRemovedPost(c, 3);
                break;
            case 5:
                test.sameItems("closing windows preserves surviving buttons");
                test.check(dock.appGroups[0].windows.length === 1, "closed window removed from group");
                dock.moveGroup("dock-test-b", 0);
                test.check(dock.appGroups[0].id === "dock-test-b", "reorder applies before drag transform clears");
                break;
            case 6:
                test.check(dock.testItems.itemAt(0) === test.savedItems[1] && dock.testItems.itemAt(1) === test.savedItems[0], "reordering moves existing buttons");
                test.check(dock.testItems.itemAt(0).width === 72 && dock.testItems.itemAt(1).width === 72, "buttons settle at full width");
                dock.draggedId = "dock-test-b";
                dock.dropIndex = 1;
                dock.dragOffset = 76;
                break;
            case 7:
                dock.finishDrag();
                break;
            case 8:
                test.check(dock.draggedId === "" && !dock.settlingDrag, "drag completes and releases its state");
                test.sameItems("drag settlement keeps the original buttons in their new order");
                const first = dock.testItems.itemAt(0);
                const second = dock.testItems.itemAt(1);
                const separation = second.mapToItem(first.parent, 0, 0).x - first.mapToItem(first.parent, 0, 0).x;
                test.check(Math.abs(separation - 76) < 0.1, "drag settlement leaves exactly one slot between icons");
                console.info(test.failures.length ? "DOCK_TEST_FAILED" : "DOCK_TEST_PASSED");
                Qt.quit();
            }
        }
    }
}
