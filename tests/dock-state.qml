import QtQuick
import QtTest
import Quickshell

ShellRoot {
    id: test
    property int step: 0
    property var savedItems: []
    property var failures: []
    property int normalLaunches: 0
    property int newWindowLaunches: 0
    property var openingFrames: []
    property var closingFrames: []
    property var reopeningItem: null
    property real closingProgress: 1
    property var launchFrames: []
    TestCase { id: input; name: "DockInteraction"; when: false }

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
        property int closeRequests: 0
        function close() { closeRequests++; }
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
        interval: 16
        running: true
        repeat: true
        onTriggered: {
            if (dock.testLaunchEffect.running)
                test.launchFrames.push({scale: dock.testLaunchEffect.scale, opacity: dock.testLaunchEffect.opacity});
            for (let i = 0; i < dock.testItems.count; i++) {
                const item = dock.testItems.itemAt(i);
                if (item.modelData.id !== "dock-test-c") continue;
                const frame = { progress: item.transitionProgress, scale: item.scale, slide: item.slideOffset, z: item.z,
                    opacity: item.opacity, width: dock.testSurface.width };
                if (item.modelData.exiting)
                    test.closingFrames.push(frame);
                else
                    test.openingFrames.push(frame);
            }
        }
    }
    Timer {
        id: steps
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            if (Theme.reducedMotion) {
                switch (test.step++) {
                case 0:
                    test.savedItems = test.identities();
                    manager.toplevels.values = [a, b, c];
                    manager.toplevels.objectInsertedPost(c, 2);
                    break;
                case 1:
                    test.check(dock.testItems.count === 3 && dock.testItems.itemAt(2).transitionProgress === 1, "reduced motion adds full-size icon");
                    dock.launch({id: "dock-test-c", desktopEntry: {execute: () => { test.normalLaunches++; }}}, false);
                    test.check(!dock.testLaunchOverlay.visible && !dock.testLaunchEffect.running, "reduced motion skips launch animation");
                    test.check(dock.testItems.itemAt(2).testMouse.cursorShape === Qt.BusyCursor, "reduced motion retains launch cursor feedback");
                    manager.toplevels.values = [a, b];
                    manager.toplevels.objectRemovedPost(c, 2);
                    break;
                case 2:
                    test.sameItems("reduced motion removes departing icon without resetting neighbours");
                    b.activate();
                    a.title = "Changed with reduced motion";
                    break;
                case 3:
                    test.sameItems("reduced motion preserves buttons across focus and title changes");
                    console.info(test.failures.length ? "DOCK_TEST_FAILED" : "DOCK_TEST_PASSED");
                    Qt.quit();
                }
                return;
            }
            switch (test.step++) {
            case 0:
                test.check(dock.appGroups.length === 2, "unfocused zero-output windows stay in dock");
                test.savedItems = test.identities();
                b.activate();
                a.title = "A changed on focus";
                dock.showTooltip(dock.testItems.itemAt(0));
                break;
            case 1:
                test.check(dock.testTooltip.margins.bottom === dock.height - dock.testSurface.y + Theme.spaceSmall, "tooltip sits four pixels above the actual dock surface");
                test.check(Math.abs(dock.testTooltip.margins.left + dock.testTooltip.width / 2 - dock.testItems.itemAt(0).mapToItem(dock.contentItem, dock.testItems.itemAt(0).width / 2, 0).x) <= 1, "tooltip centres on its dock item");
                dock.dismissTooltip();
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
                test.check(test.openingFrames.some(frame => frame.progress > 0 && frame.progress < 0.9), "opening app has intermediate animation frames");
                test.check(test.openingFrames.some(frame => frame.opacity > 0 && frame.opacity < 0.9 && frame.slide < 0 && frame.z < test.savedItems[1].z), "opening icon emerges from behind the previous item");
                test.check(test.openingFrames.every(frame => frame.scale === 1), "opening icon retains full scale");
                test.check(test.openingFrames.length > 1 && test.openingFrames[test.openingFrames.length - 1].width > test.openingFrames[0].width, "dock grows during entry");
                manager.toplevels.values = [a, b];
                manager.toplevels.objectRemovedPost(a2, 2);
                manager.toplevels.objectRemovedPost(c, 3);
                break;
            case 5:
                test.sameItems("closing windows preserves surviving buttons");
                test.check(test.closingFrames.some(frame => frame.progress > 0 && frame.progress < 0.9), "closing app has intermediate animation frames");
                test.check(test.closingFrames.some(frame => frame.opacity > 0 && frame.opacity < 0.9 && frame.slide < 0 && frame.z < test.savedItems[1].z), "closing icon retreats behind the previous item");
                test.check(test.closingFrames.every(frame => frame.scale === 1), "closing icon retains full scale");
                test.check(test.closingFrames.length > 1 && test.closingFrames[test.closingFrames.length - 1].width < test.closingFrames[0].width, "dock shrinks during departure");
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
                first.menuOpen = true;
                break;
            case 9:
                const button = dock.testItems.itemAt(0);
                const windowEntry = test.findLabel(button.testMenu.body, a.title);
                const closeButton = windowEntry.contentItem.children[0].children.find(item => item.objectName === "closeWindowButton");
                test.check(closeButton && closeButton.visible, "window entry exposes a close button");
                input.mouseClick(closeButton, closeButton.width / 2, closeButton.height / 2, Qt.LeftButton);
                test.check(a.closeRequests === 1 && b.closeRequests === 0 && !a.activated, "close targets only its window without activating it");
                test.check(button.menuOpen, "close request keeps menu open while window handles the request");
                button.menuOpen = false;
                const originalGroup = button.modelData;
                button.modelData = {
                    id: "dock-test-a", windows: [a],
                    desktopEntry: {
                        name: "Test app", icon: "application-x-executable",
                        actions: [{ id: "new-window", execute: () => { test.newWindowLaunches++; } }],
                        execute: () => { test.normalLaunches++; },
                    },
                };
                test.check((button.testMouse.acceptedButtons & Qt.MiddleButton) !== 0, "dock accepts middle clicks");
                const edgeX = button.mapToItem(dock.contentItem, button.width / 2, 0).x;
                test.check(dock.testSurface.bottomGap === Theme.padding - Theme.spaceSmall, "dock sits four pixels lower");
                input.mouseClick(dock.contentItem, edgeX, dock.height - 1, Qt.MiddleButton);
                test.check(test.newWindowLaunches === 1 && test.normalLaunches === 0, "middle click uses explicit new-window action");
                test.check(button.testMouse.cursorShape === Qt.BusyCursor, "launch shows a busy cursor on its dock item");
                button.modelData.desktopEntry.actions = [];
                input.mouseClick(button, button.width / 2, button.height / 2, Qt.MiddleButton);
                test.check(test.normalLaunches === 1, "middle click falls back to normal launch without new-window action");
                button.modelData = originalGroup;
                manager.toplevels.values = [a, b, c];
                manager.toplevels.objectInsertedPost(c, 2);
                break;
            case 10:
                test.check(test.launchFrames.some(frame => frame.scale > 1 && frame.scale < 4 && frame.opacity > 0 && frame.opacity < 1), "launch icon copy grows and fades");
                test.check(!dock.testLaunchOverlay.visible, "launch overlay closes after the effect");
                test.check(dock.testItems.itemAt(0).testMouse.cursorShape === Qt.BusyCursor, "unrelated app does not clear the launch cursor");
                manager.toplevels.values = [a, b, c, a2];
                manager.toplevels.objectInsertedPost(a2, 3);
                test.check(dock.testItems.itemAt(0).testMouse.cursorShape === Qt.ArrowCursor, "matching new window clears the launch cursor");
                test.reopeningItem = dock.testItems.itemAt(2);
                manager.toplevels.values = [a, b];
                manager.toplevels.objectRemovedPost(a2, 3);
                manager.toplevels.objectRemovedPost(c, 2);
                steps.interval = 100;
                break;
            case 11:
                test.check(test.reopeningItem.exiting && test.reopeningItem.transitionProgress > 0 && test.reopeningItem.transitionProgress < 1, "rapid reopen starts during departure");
                test.closingProgress = test.reopeningItem.transitionProgress;
                manager.toplevels.values = [a, b, c];
                manager.toplevels.objectInsertedPost(c, 2);
                break;
            case 12:
                test.check(dock.testItems.itemAt(2) === test.reopeningItem, "reopening reverses the same button");
                test.check(!test.reopeningItem.exiting && test.reopeningItem.transitionProgress > test.closingProgress, "reopening reverses departure smoothly");
                steps.interval = 500;
                break;
            case 13:
                test.check(test.reopeningItem.transitionProgress === 1, "reopened button reaches full size");
                test.check(dock.testItems.itemAt(0) === test.savedItems[0] && dock.testItems.itemAt(1) === test.savedItems[1], "reopening preserves neighbours");
                dock.moveGroup("dock-test-c", 0);
                manager.toplevels.values = [a, b];
                manager.toplevels.objectRemovedPost(c, 2);
                steps.interval = 100;
                break;
            case 14:
                test.check(test.reopeningItem.exiting && test.reopeningItem.slideOffset > 0 && test.reopeningItem.z < test.savedItems[0].z, "leftmost icon retreats behind the next item");
                test.check(test.reopeningItem.scale === 1, "left exit retains full scale");
                steps.interval = 500;
                break;
            case 15:
                test.sameItems("left exit preserves surviving buttons");
                dock.launch({id: "dock-test-a", desktopEntry: {execute: () => { test.normalLaunches++; }}}, false);
                steps.interval = 3200;
                break;
            case 16:
                test.check(dock.pendingLaunchGroupId === "" && dock.testItems.itemAt(0).testMouse.cursorShape === Qt.ArrowCursor, "launch timeout restores the default cursor");
                console.info(test.failures.length ? "DOCK_TEST_FAILED" : "DOCK_TEST_PASSED");
                Qt.quit();
            }
        }
    }
}
