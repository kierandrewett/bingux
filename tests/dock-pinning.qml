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
        function close() {}
        function activate() {}
        function setRectangle(window, rect) {}
    }
    TestWindow { id: windowB; appId: "dock-section-b" }
    TestWindow { id: windowC; appId: "dock-section-c" }
    TestWindow { id: windowD; appId: "dock-section-d" }
    QtObject {
        id: manager
        property var activeToplevel: window
        property QtObject toplevels: QtObject {
            property var values: [window]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    QtObject {
        id: window
        property string appId: "PinTest"
        property string title: "Pin test"
        property var parent: null
        property var screens: []
        property bool activated: true
        property bool minimized: false
        function close() {}
        function activate() {}
        function setRectangle(window, rect) {}
    }
    QtObject {
        id: desktop
        property string id: "dock-pin-test"
        property string startupClass: "PinTest"
        property string name: "Pin test"
        property string icon: "applications-other"
        property var actions: []
        function execute() {}
    }
    QtObject { id: config; property var pinnedApps: [] }
    Loader {
        id: loader
        sourceComponent: Component {
            Dock {
                settings: config
                testManager: manager
                function desktopEntryFor(id) {
                    if (id === "PinTest" || id === "dock-pin-test") return desktop;
                    if (id.startsWith("dock-section-"))
                        return { id: id, startupClass: id, name: id, icon: "applications-other", actions: [], execute: () => {} };
                    return null;
                }
            }
        }
    }
    TestCase {
        id: test
        parent: loader.item ? loader.item.contentItem : null
        when: loader.status === Loader.Ready
        name: "DockPinning"
        function check(value, message) { console.warn(message + ": " + value); verify(value, message); }
        function dragWithinSection(dock, source, target) {
            wait(50);
            const button = dock.testItems.itemAt(source);
            const id = button.currentGroup.id;
            const origin = button.mapToItem(dock.contentItem, button.width / 2, button.height / 2);
            const travel = (target - source) * (Theme.dockItemSize + Theme.spaceSmall);
            mousePress(dock.contentItem, origin.x, origin.y, Qt.LeftButton);
            for (let step = 1; step <= 6; step++)
                mouseMove(dock.contentItem, origin.x + travel * step / 6, origin.y, 20);
            check(dock.dropIndex === target, "Drag " + source + " to " + target + " selects slot " + dock.dropIndex + " with offset " + dock.dragOffset);
            mouseRelease(dock.contentItem, origin.x + travel, origin.y, Qt.LeftButton);
            wait(300);
            check(dock.draggedId === "", "Drag releases capture; active=" + dock.draggedId + " settling=" + dock.settlingDrag);
            check(dock.testItems.itemAt(target) === button, "Moved " + id + " reaches slot " + target + "; order=" + dock.appGroups.map(group => group.id)
                  + "; rendered=" + Array.from({length: dock.testItems.count}, (_, i) => dock.testItems.itemAt(i).currentGroup.id));
            check(dock.appGroups[target].id === id, "Pointer reorder reaches the requested slot");
            for (let index = 1; index < dock.testItems.count; index++) {
                const before = dock.testItems.itemAt(index - 1);
                const after = dock.testItems.itemAt(index);
                const distance = after.mapToItem(after.parent, 0, 0).x - before.mapToItem(before.parent, 0, 0).x;
                const expected = Theme.dockItemSize + Theme.spaceSmall + (index === dock.pinnedGroupCount ? Theme.padding : 0);
                check(Math.abs(distance - expected) < 0.1, "Released icons have consistent spacing");
            }
            const divider = findChild(dock.contentItem, "dockSectionDivider");
            check(divider.x > dock.testItems.itemAt(1).x + dock.testItems.itemAt(1).width
                  && divider.x < dock.testItems.itemAt(2).x, "Divider remains between sections after pointer reorder");
        }
        function test_pin() {
            let dock = loader.item;
            tryVerify(() => dock.testItems.count === 1);
            let menu = dock.testItems.itemAt(0).testMenu;
            menu.visible = true;
            tryCompare(menu, "revealScale", 1);
            test.parent = menu.contentItem;
            let action = findChild(menu.body, "dockPinAction");
            check(action.label === "Pin to dock" && action.enabled, "Running app offers Pin to dock");
            mouseClick(action);
            tryVerify(() => dock.pinnedApps.indexOf("dock-pin-test") >= 0);
            check(!menu.visible, "Pin action closes the menu");
            manager.toplevels.values = [];
            dock.refreshAppGroups();
            tryVerify(() => dock.appGroups[0].windows.length === 0);
            check(dock.testItems.count === 1 && !dock.appGroups[0].exiting, "Pinned app remains after its last window closes");
            loader.active = false;
            wait(50);
            loader.active = true;
            tryCompare(loader, "status", Loader.Ready);
            dock = loader.item;
            tryVerify(() => dock.testItems.count === 1);
            check(dock.isPinned(dock.appGroups[0]), "Pin persists after recreating the dock from saved settings");
            config.pinnedApps = ["dock-pin-test"];
            menu = dock.testItems.itemAt(0).testMenu;
            menu.visible = true;
            tryCompare(menu, "revealScale", 1);
            test.parent = menu.contentItem;
            action = findChild(menu.body, "dockPinAction");
            check(action.label === "Unpin from dock", "Pinned app offers Unpin from dock");
            action.forceActiveFocus();
            keyClick(Qt.Key_Return);
            tryVerify(() => dock.pinnedApps.length === 0);
            tryVerify(() => dock.testItems.count === 0);
            check(true, "Keyboard unpin removes an idle app and overrides a configured pin");
            loader.active = false;
            wait(50);
            loader.active = true;
            tryCompare(loader, "status", Loader.Ready);
            wait(100);
            check(loader.item.pinnedApps.length === 0, "Unpin preference persists across recreation");

            dock = loader.item;
            test.parent = dock.contentItem;
            config.pinnedApps = [windowB.appId, windowD.appId];
            manager.toplevels.values = [window, windowC, windowB, windowD];
            dock.refreshAppGroups();
            tryCompare(dock.testItems, "count", 4);
            tryVerify(() => dock.appGroups.slice(0, 2).every(group => dock.isPinned(group)));
            tryVerify(() => dock.testItems.itemAt(3).transitionProgress === 1);
            const divider = findChild(dock.contentItem, "dockSectionDivider");
            check(divider.visible, "Mixed dock shows a section divider");
            const gap = dock.testItems.itemAt(2).x - dock.testItems.itemAt(1).x - dock.testItems.itemAt(1).width;
            check(gap > Theme.spaceSmall && divider.x > dock.testItems.itemAt(1).x + dock.testItems.itemAt(1).width
                  && divider.x < dock.testItems.itemAt(2).x, "Divider has space between the two sections");
            const saved = dock.appGroups.map(group => group.id);
            dock.moveGroup(saved[0], 3);
            check(dock.appGroups[1].id === saved[0], "Pinned reorder stops at the section boundary");
            wait(100);
            check(divider.x > dock.testItems.itemAt(1).x + dock.testItems.itemAt(1).width
                  && divider.x < dock.testItems.itemAt(2).x, "Divider stays at the boundary after reordering pinned apps");
            dock.moveGroup(saved[3], 0);
            check(dock.appGroups[2].id === saved[3], "Running reorder stops at the section boundary");
            const order = dock.appGroups.map(group => group.id).join(",");
            loader.active = false;
            wait(50);
            loader.active = true;
            tryCompare(loader, "status", Loader.Ready);
            dock = loader.item;
            test.parent = dock.contentItem;
            tryCompare(dock.testItems, "count", 4);
            check(dock.appGroups.map(group => group.id).join(",") === order, "Order persists within both sections");
            const button = dock.testItems.itemAt(2);
            button.forceActiveFocus();
            keyClick(Qt.Key_Left, Qt.ControlModifier);
            check(dock.appGroups.map(group => group.id).join(",") === order, "Keyboard reorder cannot cross the divider");
            const pinOrigin = button.mapToItem(dock.contentItem, 36, 36);
            const pinTarget = dock.testItems.itemAt(0).mapToItem(dock.contentItem, 36, 36);
            const draggedApp = button.currentGroup.id;
            mousePress(dock.contentItem, pinOrigin.x, pinOrigin.y, Qt.LeftButton);
            mouseMove(dock.contentItem, pinTarget.x, pinTarget.y, 50);
            check(dock.dropIndex === 0, "Dragging into pinned section selects the requested slot");
            const pinBadge = findChild(button, "dockPinPreview");
            check(pinBadge && pinBadge.shown && pinBadge.iconName === "view-pin-symbolic", "Dragging into pinned section shows a pin badge");
            wait(200);
            dock.testSurface.grabToImage(result => result.saveToFile("/tmp/bingux-notification-polish/dock-pin-preview.png"));
            wait(30);
            mouseRelease(dock.contentItem, pinTarget.x, pinTarget.y, Qt.LeftButton);
            tryVerify(() => dock.draggedId === "");
            check(dock.appGroups[0].id === draggedApp && dock.isPinned(dock.appGroups[0]), "Drop pins " + draggedApp + " at its requested position; order=" + dock.appGroups.map(group => group.id) + "; pins=" + dock.pinnedApps);
            window.title = "Metadata changed after dropping";
            dock.refreshAppGroups();
            wait(100);
            check(dock.appGroups[0].id === draggedApp, "Window updates do not reset the drop");
            loader.active = false;
            wait(50);
            loader.active = true;
            tryCompare(loader, "status", Loader.Ready);
            dock = loader.item;
            test.parent = dock.contentItem;
            tryCompare(dock.testItems, "count", 4);
            check(dock.appGroups[0].id === draggedApp && dock.isPinned(dock.appGroups[0]), "Dropped position and pin survive recreating the dock");
            const unpinOrigin = dock.testItems.itemAt(0).mapToItem(dock.contentItem, 36, 36);
            const unpinTarget = dock.testItems.itemAt(3).mapToItem(dock.contentItem, 36, 36);
            mousePress(dock.contentItem, unpinOrigin.x, unpinOrigin.y, Qt.LeftButton);
            mouseMove(dock.contentItem, unpinTarget.x, unpinTarget.y, 50);
            mouseRelease(dock.contentItem, unpinTarget.x, unpinTarget.y, Qt.LeftButton);
            tryVerify(() => dock.draggedId === "");
            check(dock.appGroups[3].id === draggedApp && !dock.isPinned(dock.appGroups[3]), "Dropping back into running apps unpins and keeps the new position");
            dragWithinSection(dock, 2, 3);
            dragWithinSection(dock, 3, 2);
            dragWithinSection(dock, 0, 1);
            dragWithinSection(dock, 1, 0);
            dock.testSurface.grabToImage(result => result.saveToFile("/tmp/bingux-notification-polish/dock-sections.png"));
            wait(100);
            dock.setPinned(dock.appGroups[0], false);
            dock.setPinned(dock.appGroups[1], false);
            tryCompare(dock, "pinnedGroupCount", 0);
            check(!findChild(dock.contentItem, "dockSectionDivider").visible, "No divider when all apps are unpinned");
            config.pinnedApps = ["dock-pin-test", windowB.appId, windowC.appId, windowD.appId];
            for (const group of dock.appGroups.slice()) dock.setPinned(group, true);
            tryCompare(dock, "pinnedGroupCount", 4);
            check(!findChild(dock.contentItem, "dockSectionDivider").visible, "No divider when all apps are pinned");
            const lastPinned = dock.appGroups[3].id;
            dock.moveGroup(lastPinned, 0);
            check(dock.appGroups[0].id === lastPinned, "Reordering four pinned apps preserves the requested order");
            const finalOrder = dock.appGroups.map(group => group.id).join(",");
            for (let refresh = 0; refresh < 4; refresh++) dock.refreshAppGroupsNow();
            check(dock.appGroups.map(group => group.id).join(",") === finalOrder, "Repeated refreshes preserve the pinned order");
            loader.active = false;
            wait(50);
            loader.active = true;
            tryCompare(loader, "status", Loader.Ready);
            tryVerify(() => loader.item.appGroups.length === 4);
            check(loader.item.appGroups.map(group => group.id).join(",") === finalOrder, "Pinned order remains saved after recreation");
            console.info("DOCK_TEST_PASSED");
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
