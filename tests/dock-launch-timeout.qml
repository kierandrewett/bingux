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
    QtObject { id: otherWindow; property string appId: "unrelated-app" }
    QtObject { id: appWindow; property string appId: "slow-app" }
    Dock {
        id: dock
        testManager: manager
        settings: QtObject { property var pinnedApps: [] }
    }
    TestCase {
        name: "DockLaunchTimeout"
        when: dock.visible
        function check(condition, message) {
            if (!condition) console.error("FAIL: " + message);
            verify(condition, message);
        }
        function test_loading() {
            wait(300);
            const group = {id: "slow-app", desktopEntry: {id: "slow-app.desktop"}};
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
