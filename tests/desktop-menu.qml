import QtQuick
import QtTest
import Quickshell

// Exercises DesktopMenu's own logic: monitor and pointer placement, the action
// list, dispatch after the target probe, and the visible failure path. The
// driver replaces TrayMenu with a stub because offscreen Quickshell has no
// layer-shell backend for ShellPopup.
ShellRoot {
    id: testRoot

    property var failures: []
    property var dispatched: []
    readonly property string scenario: Quickshell.env("BINGUX_DESKTOP_MENU_SCENARIO")

    DesktopMenu {
        id: menu
        onRequestFailed: message => {
            testRoot.failures = testRoot.failures.concat([message]);
        }
        onDispatched: program => {
            testRoot.dispatched = testRoot.dispatched.concat([program]);
        }
    }

    function actionNamed(text) {
        return menu.actions.find(action => action.text === text);
    }

    TestCase {
        name: "DesktopMenu"
        when: true

        function test_open_placement_and_actions() {
            const screen = Quickshell.screens[0];
            verify(screen, "A screen is required");
            menu.openAt(screen, 240, 160);
            compare(menu.visible, true, "openAt shows the menu");
            compare(menu.screen, screen, "The requested monitor owns the menu");
            compare(menu.requestedPosition.x, 240);
            compare(menu.requestedPosition.y, 160);
            compare(menu.preferredX, 240, "Pointer x is the preferred placement");
            compare(menu.preferredY, 160, "Pointer y is the preferred placement");
            compare(menu.actions.map(action => action.text).join("|"), "Open Terminal|Bingux Settings|Display Settings|Change Background…", "Desktop menu exposes the four actions");
            menu.openAt(screen, NaN, 10);
            compare(menu.requestedPosition.x, 240, "Non-finite coordinates are ignored");
        }

        function test_dispatch_or_visible_failure() {
            const screen = Quickshell.screens[0];
            if (scenario === "dispatch") {
                menu.openAt(screen, 240, 160);
                actionNamed("Display Settings").triggered();
                tryVerify(() => testRoot.dispatched.length === 1, 5000);
                console.info("DESKTOP_MENU_DISPATCH_PASSED");
            } else if (scenario === "missing") {
                menu.openAt(screen, 240, 160);
                actionNamed("Bingux Settings").triggered();
                tryVerify(() => testRoot.failures.some(message => message.includes("Could not open Bingux Settings")), 5000);
                compare(testRoot.dispatched.length, 0, "A missing target is not dispatched");
                console.info("DESKTOP_MENU_MISSING_PASSED");
            } else {
                verify(false, "Unknown BINGUX_DESKTOP_MENU_SCENARIO");
            }
        }

        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
