import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

ShellRoot {
    WorkspaceState {
        id: workspaces
    }
    WorkspaceChangeState {
        id: state
        source: workspaces
    }
    WorkspaceChangeSurface {
        id: surface
        state: state
    }
    TestCase {
        name: "WorkspaceChange"
        when: surface.testWindows.length > 0
        function check(value, message) {
            verify(value, message);
        }
        function findChild(item, name) {
            if (item.objectName === name)
                return item;
            for (let i = 0; i < item.children.length; i++) {
                const found = findChild(item.children[i], name);
                if (found)
                    return found;
            }
            return null;
        }
        function ingest(record) {
            workspaces.ingest(JSON.stringify(record));
        }
        function activate(number, name, sequence) {
            ingest({
                event: "gnoblin.workspace.activated",
                workspace: {
                    id: "ws-" + number,
                    number: number,
                    name: name,
                    active: true
                },
                previous_id: "",
                sequence: sequence
            });
        }
        function shownWindows() {
            return surface.testWindows.filter(window => window.active);
        }
        function allHidden() {
            return surface.testWindows.every(window => !window.visible);
        }
        function test_feedback() {
            check(!state.shown && allHidden(), "Nothing is shown before a change");
            const focusWindow = surface.testWindows[0];
            check(!focusWindow.focusable && focusWindow.WlrLayershell.keyboardFocus === WlrKeyboardFocus.None && focusWindow.testMask.width === 0 && focusWindow.testMask.height === 0, "Feedback never takes keyboard or pointer input");

            // A completed workspace change reported by the compositor names the new workspace.
            activate(2, "Code", 10);
            check(state.shown && state.workspace.number === 2 && state.workspace.name === "Code" && state.sequence === 10, "Completed activation updates the state");
            check(shownWindows().length === 1, "Exactly one output shows the feedback");
            const card = shownWindows()[0].testCard;
            if (Theme.reducedMotion) {
                check(Theme.popupOpenMotion === 0 && Theme.osdCloseMotion === 0, "Reduced motion uses zero-length transitions");
                tryCompare(card, "opacity", 1, 200);
            } else {
                check(card.opacity < 1, "The label fades in");
                wait(60);
                check(card.opacity > 0 && card.opacity < 1, "The fade-in is partway through the animation");
                tryCompare(card, "opacity", 1);
            }
            check(findChild(card, "workspaceChangeNumber").text === "Workspace 2" && findChild(card, "workspaceChangeName").text === "Code", "The label names the workspace");
            check(card.Accessible.name === "Workspace 2: Code", "The label is announced with its name");

            // Rapid changes replace the label without stacking.
            activate(3, "Mail", 11);
            activate(4, "Music", 12);
            activate(5, "Notes", 13);
            check(state.workspace.number === 5 && state.sequence === 13 && shownWindows().length === 1, "Rapid changes update one label");
            check(findChild(shownWindows()[0].testCard, "workspaceChangeName").text === "Notes", "The label shows the latest workspace");

            // Late or malformed reports do not replace the current label.
            activate(1, "Old", 3);
            ingest({
                event: "gnoblin.workspace.activated",
                workspace: {
                    id: "bad",
                    number: 0,
                    name: "Broken",
                    active: true
                },
                sequence: 14
            });
            check(state.workspace.number === 5 && state.sequence === 13, "Stale and malformed activations are ignored");

            // Lifecycle and window events are not completed switches.
            ingest({
                event: "gnoblin.workspace.created",
                workspace: {
                    id: "ws-9",
                    number: 9,
                    name: "New",
                    active: false
                },
                sequence: 15
            });
            ingest({
                event: "windows",
                windows: []
            });
            check(state.workspace.number === 5 && state.sequence === 13, "Lifecycle and window events do not show feedback");

            // Each change restarts the hold, so the label stays while switching continues.
            wait(800);
            activate(6, "Chat", 16);
            wait(800);
            check(state.shown && state.workspace.number === 6, "A new change restarts the hold");

            // Expiry hides the label, and nothing stays mapped afterwards.
            tryVerify(() => !state.shown, 2000);
            tryVerify(() => allHidden(), 2000);
            check(!state.shown && allHidden(), "The label expires and unmaps");
            console.info("WORKSPACE_CHANGE_TEST_PASSED");
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
