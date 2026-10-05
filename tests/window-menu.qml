import QtQuick
import QtTest
import Quickshell

ShellRoot {
    id: testRoot

    property var failures: []

    WindowMenu {
        id: menu
    }

    Connections {
        target: menu
        function onRequestFailed(message) {
            testRoot.failures = testRoot.failures.concat([message]);
        }
    }

    TestCase {
        name: "NativeWindowMenu"
        when: true

        function test_event_and_action_path() {
            const scenario = Quickshell.env("BINGUX_WINDOW_MENU_SCENARIO");
            if (scenario === "stale") {
                tryVerify(() => testRoot.failures.some(message => message.includes("no longer exists")), 5000);
                compare(menu.lastMenuRequestType, "wm");
                verify(!menu.visible, "A stale target must not open the menu");
                console.info("WINDOW_MENU_STALE_PASSED");
            } else if (scenario === "live") {
                tryVerify(() => menu.visible, 5000);
                compare(menu.windowId, "101");
                compare(menu.lastMenuRequestType, "wm");
                compare(menu.requestedPosition.x, 120);
                compare(menu.requestedPosition.y, 80);
                verify(menu.actions.some(action => action.text === "Move"), "WM token enables move");
                verify(menu.actions.some(action => action.text === "Resize from bottom-right"), "WM token enables resize");

                const move = menu.actions.find(action => action.text === "Move");
                move.triggered();
                const alwaysOnTop = menu.actions.find(action => action.text === "Always on Top");
                verify(alwaysOnTop, "Always on Top action exists");
                menu.activate(alwaysOnTop);
                tryVerify(() => testRoot.failures.some(message => message.includes("window.set_above failed")), 5000);

                tryVerify(() => menu.lastMenuRequestType === "app", 5000);
                verify(!menu.visible, "Informational app-menu requests do not open a WM menu");
                compare(menu.windowId, "");
                compare(menu.actions.length, 0);
                console.info("WINDOW_MENU_LIVE_PASSED");
            } else {
                verify(false, "Unknown BINGUX_WINDOW_MENU_SCENARIO");
            }
        }

        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
