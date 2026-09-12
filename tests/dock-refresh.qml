import QtQuick
import QtTest
import Quickshell

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
        appId: "gnoblin-perf-a"
    }
    TestWindow {
        id: b
        appId: "gnoblin-perf-b"
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
        function test_refresh() {
            tryCompare(dock.testItems, "count", 2);
            wait(300);
            const groups = dock.appGroups;
            for (let i = 0; i < 50; ++i) {
                a.title = "Progress " + i;
                wait(20);
            }
            verify(dock.appGroups === groups, "Title updates must not rebuild or sort the dock");
            compare(dock.appGroups[0].windows[0].title, "Progress 49");
            dock.desktopEntryFor("gnoblin-perf-uninstalled");
            verify(dock.desktopEntryCache.has("gnoblin-perf-uninstalled"), "Cache failed lookups too");
            const cache = dock.desktopEntryCache;
            dock.desktopEntryFor("gnoblin-perf-uninstalled");
            compare(dock.desktopEntryCache, cache);
            dock.moveGroup("gnoblin-perf-b", 0);
            tryVerify(() => dock.appGroups[0].id === "gnoblin-perf-b");
            a.appId = "gnoblin-perf-c";
            tryVerify(() => dock.appGroups.some(group => group.id === "gnoblin-perf-c"));
            console.info("DOCK_TEST_PASSED");
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
