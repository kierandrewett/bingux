import QtQuick
import QtTest
import Quickshell

ShellRoot {
    component TestWindow: QtObject {
        property string appId: "com.mitchellh.ghostty"
        property string title: "Terminal"
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        function activate() {
            for (const window of manager.toplevels.values)
                window.activated = window === this;
            manager.activeToplevel = this;
        }
        function setRectangle(window, rectangle) {
        }
    }
    TestWindow {
        id: first
        title: "First terminal"
    }
    TestWindow {
        id: second
        title: "Second terminal"
        activated: true
    }
    TestWindow {
        id: other
        appId: "other-app"
    }
    QtObject {
        id: manager
        property var activeToplevel: second
        property QtObject toplevels: QtObject {
            property var values: [first, second, other]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    Dock {
        id: dock
        testManager: manager
        settings: QtObject {
            property var pinnedApps: []
        }
    }
    TestCase {
        when: dock.visible
        function test_focus_indicators() {
            try {
                tryCompare(dock.testItems, "count", 2);
                const button = Array.from({
                    length: dock.testItems.count
                }, (_, i) => dock.testItems.itemAt(i)).find(item => item.currentGroup.id === first.appId);
                const dots = button.testIndicators;
                const pointer = button.testMouse;
                for (const delta of [-120, 120]) {
                    first.minimized = false;
                    second.minimized = false;
                    second.activate();
                    tryCompare(dots, "activeIndex", 1);
                    other.activate();
                    tryCompare(dots, "activeIndex", -1);
                    wait(180);
                    mouseWheel(pointer, 20, 20, 0, delta);
                    tryCompare(manager, "activeToplevel", second, 500, "First scroll after focus loss must reveal the remembered terminal");
                    tryCompare(dots, "activeIndex", 1);
                    wait(180);
                    mouseWheel(pointer, 20, 20, 0, delta);
                    tryCompare(manager, "activeToplevel", first);
                    tryCompare(dots, "activeIndex", 0);
                    // Explicit minimise must select the same restore target.
                    second.activate();
                    second.minimized = true;
                    other.activate();
                    wait(180);
                    mouseWheel(pointer, 20, 20, 0, delta);
                    tryCompare(manager, "activeToplevel", second);
                    verify(!second.minimized);
                    tryCompare(dots, "activeIndex", 1);
                }
                console.info("DOCK_TEST_PASSED");
            } catch (error) {
                console.error("DOCK_BEHAVIOUR_FAILED", error.message, error.stack);
            }
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
