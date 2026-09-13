import QtQuick
import QtTest
import Quickshell

ShellRoot {
    QtObject {
        id: manager
        property var activeToplevel: null
        property QtObject toplevels: QtObject {
            property var values: []
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    QtObject {
        id: otherWindow
        property string appId: "unrelated-app"
    }
    QtObject {
        id: appWindow
        property string appId: "slow-app"
    }
    Dock {
        id: dock
        testManager: manager
        settings: QtObject {
            property var pinnedApps: []
        }
    }
    TestCase {
        name: "DockLaunchTimeout"
        when: dock.visible
        function check(condition, message) {
            if (!condition)
                console.error("FAIL: " + message);
            verify(condition, message);
        }
        function findLabel(item, label) {
            if (item && item.label === label)
                return item;
            for (const child of item?.children || []) {
                const found = findLabel(child, label);
                if (found)
                    return found;
            }
            return null;
        }
        function test_loading() {
            wait(300);
            const group = {
                id: "slow-app",
                windows: [],
                desktopEntry: {
                    id: "slow-app.desktop"
                }
            };
            dock.appGroups = [group];
            dock.launch(group, false);
            wait(100);
            const menu = dock.testItems.itemAt(0).testMenu;
            menu.visible = true;
            tryCompare(menu, "revealScale", 1);
            const loading = findLabel(menu.body, "Loading...");
            check(loading && loading.loading, "Open windows shows a loading entry");
            const cancel = loading.contentItem.children[0].children.find(item => item.objectName === "closeWindowButton");
            check(cancel && cancel.visible, "Loading entry exposes a cancel button");
            mouseClick(cancel, cancel.width / 2, cancel.height / 2, Qt.LeftButton);
            check(!dock.launchAttempts[group.id] && dock.pendingLaunchGroupId === "" && LaunchFeedback.active.length === 0, "Cancel clears the pending launch and feedback");
            menu.visible = false;

            // Continue with the timeout coverage using a fresh launch.
            dock.launch(group, false);
            wait(3300);
            check(dock.pendingLaunchGroupId === group.id, "Loading must survive the old three-second timeout and successful launcher exit");
            dock.associatePendingLaunchToplevel(otherWindow);
            check(dock.pendingLaunchGroupId === group.id, "An unrelated window must not complete the launch");
            dock.associatePendingLaunchToplevel(appWindow);
            check(dock.pendingLaunchGroupId === "", "The matching app window completes the launch immediately");
            dock.launch(group, false);
            wait(19000);
            check(dock.pendingLaunchGroupId === group.id, "Loading remains active before twenty seconds");
            wait(1200);
            check(dock.pendingLaunchGroupId === "", "Loading clears after the twenty-second timeout");
            console.warn("DOCK_TEST_PASSED: delayed window, unrelated window and 20-second timeout");
        }
    }
}
