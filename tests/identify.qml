import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    IdentifyState {
        id: state
        holdDuration: 600
    }
    IdentifySurface {
        id: surface
        state: state
    }
    Process {
        id: swaymsg
    }
    TestCase {
        name: "Identify"
        when: surface.identifyWindows.instances.length === 2

        function windows() {
            return surface.identifyWindows.instances;
        }
        function windowFor(connector) {
            return windows().find(window => window.modelData.name === connector);
        }
        function cardFor(connector) {
            return findChild(windowFor(connector).contentItem, "identifyCard");
        }
        function activeWindows() {
            return windows().filter(window => window.active);
        }
        function allHidden() {
            return windows().every(window => !window.visible);
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
        function check(value, message) {
            verify(value, message);
        }
        function request(connectors) {
            return JSON.parse(state.show(JSON.stringify(connectors)));
        }
        // swaymsg reaches the headless compositor through the socket the runner exports.
        function setOutput(connector, enabled) {
            swaymsg.command = ["swaymsg", "output", connector, enabled ? "enable" : "disable"];
            swaymsg.running = true;
        }
        function test_identify() {
            // Nothing is shown before a request, and every label is inert.
            check(!state.shown && activeWindows().length === 0 && allHidden(), "Nothing is shown before a request");
            for (const window of windows()) {
                check(!window.focusable && window.WlrLayershell.keyboardFocus === WlrKeyboardFocus.None, "Labels never take keyboard focus");
                check(window.mask.item.width === 0 && window.mask.item.height === 0, "Labels are click-through");
            }

            // Each output shows the position it has in the request, not its own index.
            let result = request(["HEADLESS-2", "HEADLESS-1"]);
            check(result.ok === true && result.labelled.length === 2, "A request labels both outputs");
            check(activeWindows().length === 2, "Every named output shows a label");
            check(windowFor("HEADLESS-2").number === 1 && windowFor("HEADLESS-1").number === 2, "Numbers follow the request order");
            tryCompare(cardFor("HEADLESS-2"), "opacity", 1, 2000);
            const secondCard = cardFor("HEADLESS-2");
            check(findChild(secondCard, "identifyNumber").text === "1" && findChild(secondCard, "identifyConnector").text === "HEADLESS-2", "The label shows the number and connector");
            check(secondCard.Accessible.name === "Display 1: HEADLESS-2", "The label is announced with its number and connector");

            // A new request replaces the labels rather than stacking them.
            request(["HEADLESS-1"]);
            check(activeWindows().length === 1 && windowFor("HEADLESS-1").number === 1 && state.labels.length === 1, "A new request replaces earlier labels");
            check(!windowFor("HEADLESS-2").active, "An output left out of the newest request loses its label");

            // A connector with no output keeps its position but gets no label.
            request(["NOPE-9", "HEADLESS-2"]);
            check(state.numberFor("NOPE-9") === 0 && state.numberFor("HEADLESS-2") === 2, "Unknown connectors take a position but show no label");
            check(activeWindows().length === 1 && windowFor("HEADLESS-2").number === 2, "Only the present output is labelled");

            // Malformed requests are rejected and leave the current labels alone.
            for (const payload of ["", "not json", "[]", "{}", "[\"A\",\"A\"]", "[\"bad name\"]", "[\"\"]", "[1]"]) {
                const rejected = JSON.parse(state.show(payload));
                check(rejected.ok === false && rejected.error.length > 0, "Rejects " + payload);
            }
            check(state.numberFor("HEADLESS-2") === 2 && state.labels.length === 2, "Rejected requests keep the current labels");

            // Hide clears labels at once.
            check(JSON.parse(state.hide()).ok === true, "Hide reports success");
            check(!state.shown && activeWindows().length === 0, "Hide removes every label");
            tryVerify(() => allHidden(), 2000);

            // Expiry clears a label after the hold without any further request.
            request(["HEADLESS-1"]);
            check(state.shown, "A label is visible after a request");
            tryVerify(() => !state.shown, 2000);
            tryVerify(() => allHidden(), 2000);
            check(!state.shown && allHidden(), "The label expires and unmaps");

            // A repeated request restarts the hold instead of keeping the old deadline.
            request(["HEADLESS-1"]);
            wait(400);
            request(["HEADLESS-1"]);
            wait(400);
            check(state.shown && activeWindows().length === 1, "A repeated request restarts the hold");
            tryVerify(() => !state.shown, 2000);

            // Hotplug: a long hold, so only losing the output can clear the labels.
            state.holdDuration = 60000;
            request(["HEADLESS-1", "HEADLESS-2"]);
            check(activeWindows().length === 2, "Both labels are up before hotplug");
            setOutput("HEADLESS-2", false);
            tryVerify(() => !state.shown, 3000);
            tryVerify(() => allHidden(), 2000);
            check(state.numberFor("HEADLESS-1") === 0 && activeWindows().length === 0, "Losing an output clears every label");
            state.holdDuration = 600;

            // Once the output returns, a fresh request labels it again.
            setOutput("HEADLESS-2", true);
            tryVerify(() => windowFor("HEADLESS-2") !== undefined, 3000);
            request(["HEADLESS-2"]);
            check(activeWindows().length === 1 && windowFor("HEADLESS-2").number === 1, "A later request labels the returned output");
            tryVerify(() => !state.shown, 2000);
            console.info("IDENTIFY_TEST_PASSED");
        }
        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
