import QtQuick
import QtTest
import Quickshell

// Exercises DockWindowMenu against a bridge that records Gnoblin calls. Checks
// the actions offered for each window state, exact-id dispatch, the refusal
// when a window cannot be identified, and the error row after a failed call.
ShellRoot {
    id: testRoot

    property var calls: []
    property var lookupResult: ({})
    property string failMethod: ""
    readonly property var liveWindow: ({
            id: "7",
            record: {
                minimized: false,
                maximized: false,
                fullscreen: false,
                minimizable: true,
                maximizable: true,
                closable: true,
                sticky: false,
                workspace_number: 1
            },
            workspaces: [
                {
                    number: 1,
                    name: "One"
                },
                {
                    number: 2,
                    name: "Two"
                }
            ]
        })

    QtObject {
        id: bridge

        function lookup(toplevel, callback) {
            const result = testRoot.lookupResult;
            Qt.callLater(callback, result.match ?? null, result.reason ?? "");
        }

        function dispatch(method, arguments_, callback) {
            testRoot.calls = testRoot.calls.concat([
                {
                    method,
                    arguments: arguments_
                }
            ]);
            Qt.callLater(callback, method === testRoot.failMethod ? method + " failed: boom" : "");
        }
    }

    DockWindowMenu {
        id: menu
        bridge: bridge
        screen: Quickshell.screens[0]
    }

    function enabledLabels() {
        return menu.actions.filter(action => !action.isSeparator).map(action => action.text).join("|");
    }

    function actionNamed(text) {
        return menu.actions.find(action => action.text === text);
    }

    TestCase {
        name: "DockWindowActions"
        when: true
        property bool passed: false
        property string stage: "start"

        function check(value, message) {
            if (!value)
                console.error("DOCK_BEHAVIOUR_FAILED", message);
            verify(value, message);
        }

        function test_live_window_offers_state_valid_actions() {
            stage = "live-window";
            testRoot.lookupResult = {
                match: testRoot.liveWindow
            };
            menu.openFor({
                appId: "foot",
                title: "Terminal"
            }, 10, 10, "");
            tryVerify(() => menu.actions.length > 0, 2000);
            check(enabledLabels() === "Minimise|Maximise|Fullscreen|Move to Two|Close", "Live window lists state-valid actions, got " + enabledLabels());
            check(menu.actions.every(action => action.isSeparator || action.enabled), "Live window entries are enabled");
        }

        function test_close_dispatches_to_exact_id() {
            stage = "close";
            testRoot.calls = [];
            actionNamed("Close").triggered();
            tryVerify(() => testRoot.calls.length === 1, 2000);
            check(testRoot.calls[0].method === "window.close", "Close calls window.close");
            check(testRoot.calls[0].arguments.id === "7", "Close targets the matched Gnoblin id");
        }

        function test_minimised_window_offers_restore() {
            stage = "minimised";
            testRoot.lookupResult = {
                match: Object.assign({}, testRoot.liveWindow, {
                    record: Object.assign({}, testRoot.liveWindow.record, {
                        minimized: true
                    })
                })
            };
            menu.openFor({
                appId: "foot",
                title: "Terminal"
            }, 10, 10, "");
            tryVerify(() => menu.actions.length > 0 && enabledLabels().startsWith("Restore"), 2000);
            check(enabledLabels() === "Restore|Move to Two|Close", "Minimised window offers Restore, got " + enabledLabels());
        }

        function test_ambiguous_window_is_refused() {
            stage = "ambiguous";
            testRoot.lookupResult = {
                reason: "Window matches more than one Gnoblin window"
            };
            menu.openFor({
                appId: "foot",
                title: "Terminal"
            }, 10, 10, "");
            tryVerify(() => menu.actions.length > 0 && menu.actions[0].text.includes("more than one"), 2000);
            check(menu.actions.every(action => action.isSeparator || !action.enabled), "Ambiguous window offers no enabled action");
        }

        function test_failed_dispatch_reopens_with_error() {
            stage = "failure";
            testRoot.failMethod = "window.close";
            testRoot.lookupResult = {
                match: testRoot.liveWindow
            };
            menu.openFor({
                appId: "foot",
                title: "Terminal"
            }, 10, 10, "");
            tryVerify(() => menu.actions.length > 0 && enabledLabels().includes("Close"), 2000);
            actionNamed("Close").triggered();
            tryVerify(() => menu.actions[0].text === "window.close failed: boom", 2000);
            check(!menu.actions[0].enabled, "Error row is not actionable");
            check(enabledLabels().includes("Close"), "Failed action remains available to retry");
            passed = true;
        }

        function cleanupTestCase() {
            if (!passed)
                console.error("DOCK_BEHAVIOUR_FAILED", "Incomplete at", stage);
            else
                console.warn("DOCK_TEST_PASSED");
            Qt.quit();
        }
    }
}
