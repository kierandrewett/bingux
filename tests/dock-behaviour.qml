import QtQuick
import QtTest
import Quickshell
ShellRoot {
    component TestWindow: QtObject {
        property string appId: "behaviour-test"
        property string title: "Test window"
        property var parent: null
        property var screens: []
        property bool activated: false
        property bool minimized: false
        property int closeRequests: 0
        function activate() { for (const window of manager.toplevels.values) window.activated = window === this; manager.activeToplevel = this; }
        function close() { closeRequests++; }
        function setRectangle(window, rectangle) {}
    }
    TestWindow { id: first; activated: true }
    TestWindow { id: second }
    TestWindow { id: third }
    QtObject {
        id: manager
        property var activeToplevel: first
        property QtObject toplevels: QtObject {
            property var values: [first, second, third]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    Dock { id: dock; testManager: manager; settings: QtObject { property var pinnedApps: [] } }
    TestCase {
        when: dock.visible
        function setOption(key, value) { BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {desktop: Object.assign({}, BinguxPreferences.data.desktop, {[key]: value})}); }
        function test_behaviour() {
            try {
                tryCompare(dock.testItems, 'count', 1);
                const pointer = dock.testItems.itemAt(0).testMouse;
                setOption('dockClick', 'focus');
                mouseClick(pointer); verify(!first.minimized, 'Always focus does not minimise the current window');
                setOption('dockClick', 'toggle');
                mouseClick(pointer); verify(first.minimized && second.minimized && third.minimized, 'Toggle minimises the app');
                setOption('dockClick', 'focus');
                mouseClick(pointer); verify(!first.minimized, 'Focus restores the preferred window');
                setOption('dockMiddleClick', 'none');
                mouseClick(pointer, 20, 20, Qt.MiddleButton); compare(first.closeRequests, 0);
                setOption('dockMiddleClick', 'close');
                mouseClick(pointer, 20, 20, Qt.MiddleButton); compare(first.closeRequests, 1); compare(second.closeRequests, 1); compare(third.closeRequests, 1);
                setOption('dockScroll', 'none');
                mouseWheel(pointer, 20, 20, 0, -120); compare(manager.activeToplevel, first);
                setOption('dockScroll', 'cycle');
                mouseWheel(pointer, 20, 20, 0, -120); compare(manager.activeToplevel, second);
                mouseWheel(pointer, 20, 20, 0, -120); compare(manager.activeToplevel, second, 'A trackpad burst does not cycle repeatedly');
                wait(160);
                setOption('dockScrollDirection', 'reverse');
                mouseWheel(pointer, 20, 20, 0, -120); compare(manager.activeToplevel, first);
                setOption('dockSize', 40); tryCompare(dock, 'iconSize', 40); compare(dock.itemSize, 56);
                setOption('dockAlignment', 'left'); tryCompare(dock.testSurface, 'x', Theme.padding);
                setOption('dockAlignment', 'right'); tryVerify(() => Math.abs(dock.testSurface.x + dock.testSurface.width - dock.width + Theme.padding) < 1);
                console.log('DOCK_TEST_PASSED');
            } catch (error) { console.error('DOCK_BEHAVIOUR_FAILED', error.message, error.stack); }
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
