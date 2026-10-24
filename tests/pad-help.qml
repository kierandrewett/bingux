import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Drives the production PadHelpState and PadHelpSurface. PAD_HELP_MODE picks
// the path: "ingest" feeds records straight into the state, "socket" connects
// to the fake statusd served by tests/pad-help.py and follows its steps.
ShellRoot {
    id: root

    readonly property string mode: Quickshell.env("PAD_HELP_MODE")

    PadHelpState {
        id: state
    }
    PadHelpSurface {
        id: surface
        state: state
    }
    TestCase {
        name: "PadHelp"
        when: surface.padHelpWindows.instances.length === 2

        function windows() {
            return surface.padHelpWindows.instances;
        }
        function windowFor(connector) {
            return windows().find(window => window.modelData.name === connector);
        }
        function activeWindows() {
            return windows().filter(window => window.request !== null);
        }
        function cardFor(connector) {
            return findChild(windowFor(connector).contentItem, "padHelpCard");
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
        function countText(item, text) {
            let count = typeof item.text === "string" && item.text.indexOf(text) >= 0 ? 1 : 0;
            for (let i = 0; i < item.children.length; i++)
                count += countText(item.children[i], text);
            return count;
        }
        function record(name, outputNames, monitorIndex) {
            return {
                protocolVersion: 1,
                type: "pad-help",
                monitorIndex: monitorIndex,
                outputNames: outputNames,
                editionMode: false,
                device: {
                    id: "pad-1",
                    name: name,
                    deviceType: "pad"
                },
                buttons: [
                    {
                        button: 0
                    },
                    {
                        button: 1,
                        label: "Toggle overlay",
                        modeGroup: 0
                    }
                ],
                modeGroups: [
                    {
                        group: 0,
                        modeCount: 2,
                        switchButtons: [1]
                    }
                ],
                features: [
                    {
                        type: "ring",
                        index: 0
                    }
                ]
            };
        }
        function ingest(value) {
            return state.ingest(JSON.stringify(value));
        }
        function requireMode(expected) {
            if (root.mode !== expected)
                skip("needs PAD_HELP_MODE=" + expected);
        }
        function check(value, message) {
            console.info((value ? "PAD_HELP_OK " : "PAD_HELP_NOT_OK ") + message);
            verify(value, message);
        }
        // The fixture only uses HEADLESS-1 and HEADLESS-2, and the request names
        // one of them, so exactly that output must show the card.
        function onlyShownOn(connector) {
            const shown = activeWindows();
            return shown.length === 1 && shown[0].modelData.name === connector;
        }

        function test_ingest() {
            requireMode("ingest");

            check(state.request === null && activeWindows().length === 0, "Nothing is shown before a request");
            check(state.requestTimeout === 10000, "The default expiry is 10 seconds");

            // A record with unlabelled controls shows the device, every button and
            // the feature, and says so where a label is missing.
            check(ingest(record("Fixture pad", ["HEADLESS-2"], 1)), "A valid record is accepted");
            check(onlyShownOn("HEADLESS-2"), "One overlay appears on the mapped output");
            tryCompare(cardFor("HEADLESS-2"), "opacity", 1, 2000);
            const card = cardFor("HEADLESS-2");
            check(findChild(card, "padHelpDevice").text === "Fixture pad", "The card names the device");
            check(countText(card, "No label reported") === 3, "Missing labels are reported, not blank: " + countText(card, "No label reported"));
            check(countText(card, "Toggle overlay") === 1, "Present labels are shown");
            check(card.Accessible.name === "Pad help for Fixture pad", "The overlay is announced with its device");

            // A second request replaces the first; overlays never stack.
            check(ingest(record("Replacement pad", ["HEADLESS-1"], 2)), "A replacement record is accepted");
            check(onlyShownOn("HEADLESS-1") && state.request.device.name === "Replacement pad", "A new request replaces the overlay");
            check(windowFor("HEADLESS-2").request === null, "The earlier output is cleared by the replacement");

            // A request naming only an output that is not connected is held but drawn nowhere.
            check(ingest(record("Absent pad", ["NOPE-9"], 7)), "A record for an absent output is accepted");
            check(activeWindows().length === 0 && state.targetOutputName === "", "No overlay is drawn for an absent output");

            // Escape dismisses the card while it has keyboard focus.
            check(ingest(record("Escape pad", ["HEADLESS-2"], 1)), "A record for Escape is accepted");
            tryVerify(() => findChild(cardFor("HEADLESS-2"), "padHelpKeys").activeFocus, 2000, "The card takes keyboard focus");
            console.info("PAD_HELP_STEP press-escape");
            tryCompare(state, "request", null, 3000);
            check(activeWindows().length === 0, "Escape closes the overlay");
            tryCompare(cardFor("HEADLESS-2"), "opacity", 0, 2000);

            // A clear record closes the overlay.
            check(ingest(record("Clear pad", ["HEADLESS-1"], 2)), "A record before clear is accepted");
            check(state.request !== null, "The overlay is up before clear");
            check(state.ingest(JSON.stringify({ protocolVersion: 1, type: "pad-help-clear" })), "A clear record is accepted");
            check(state.request === null && activeWindows().length === 0, "A clear record closes the overlay");

            // Expiry clears a request without any further record.
            state.requestTimeout = 800;
            check(ingest(record("Expiry pad", ["HEADLESS-1"], 2)), "A record for expiry is accepted");
            tryCompare(state, "request", null, 3000);
            check(activeWindows().length === 0, "The overlay expires and unmaps");
            state.requestTimeout = 10000;

            // A malformed record fails closed: the overlay clears, the record is refused,
            // and the connection is re-established before further records are read.
            check(ingest(record("Before malformed", ["HEADLESS-1"], 2)), "A record before a malformed one is accepted");
            check(state.ingest("{\"protocolVersion\":1,\"type\":\"pad-help\"}") === false, "A malformed record is refused");
            check(state.request === null && activeWindows().length === 0, "A malformed record clears the overlay");
            check(state.rejectingConnection === true, "A malformed record drops the connection");
            tryCompare(state, "rejectingConnection", false, 4000);
            check(ingest(record("After malformed", ["HEADLESS-1"], 2)) && state.request !== null, "Records resume after reconnecting");
            check(state.ingest("x".repeat(64 * 1024 + 1)) === false, "An oversized record is refused");
            tryCompare(state, "rejectingConnection", false, 4000);
            console.info("PAD_HELP_TEST_PASSED ingest");
        }

        function test_socket() {
            requireMode("socket");
            state.requestTimeout = 4000;

            // First connection: the fixture request is shown on HEADLESS-2.
            tryVerify(() => state.request !== null && state.request.device.name === "Fixture pad", 5000, "The first request arrives over the socket");
            check(onlyShownOn("HEADLESS-2"), "The socket request is drawn on its output only");
            console.info("PAD_HELP_STEP await-replace");

            // A replacement on another output replaces the overlay.
            tryVerify(() => state.request !== null && state.request.device.name === "Replacement pad", 5000, "The replacement arrives over the socket");
            check(onlyShownOn("HEADLESS-1"), "The replacement moves the overlay to its output");
            console.info("PAD_HELP_STEP await-disconnect");

            // Losing the bridge clears the overlay, and the state reconnects for the next request.
            tryCompare(state, "request", null, 5000);
            check(activeWindows().length === 0, "A dropped bridge clears the overlay");
            tryVerify(() => state.request !== null && state.request.device.name === "Reconnected pad", 5000, "A reconnect receives the next request");
            check(onlyShownOn("HEADLESS-2"), "The reconnected request is drawn");
            console.info("PAD_HELP_STEP await-clear");

            // A clear record from the bridge closes the overlay.
            tryCompare(state, "request", null, 5000);
            check(activeWindows().length === 0, "A clear record closes the overlay");
            console.info("PAD_HELP_STEP await-escape");

            // Escape closes a request received over the socket.
            tryVerify(() => state.request !== null && state.request.device.name === "Escape pad", 5000, "The Escape request arrives");
            tryVerify(() => findChild(cardFor("HEADLESS-2"), "padHelpKeys").activeFocus, 2000, "The card takes keyboard focus");
            console.info("PAD_HELP_STEP press-escape");
            tryCompare(state, "request", null, 3000);
            check(activeWindows().length === 0, "Escape closes a socket request");
            console.info("PAD_HELP_STEP await-expiry");

            // With no further records, a request expires after the configured hold.
            tryVerify(() => state.request !== null && state.request.device.name === "Expiry pad", 5000, "The expiry request arrives");
            tryCompare(state, "request", null, 5000);
            check(activeWindows().length === 0, "A socket request expires");
            console.info("PAD_HELP_TEST_PASSED socket");
        }

        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
