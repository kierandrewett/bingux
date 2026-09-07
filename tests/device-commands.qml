import QtQuick
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: test
    property int failures: 0
    property int phase: 0
    property double started: Date.now()
    property string uuid: "00000000-1111-2222-3333-444444444444"
    property string failedUuid: "00000000-1111-2222-3333-555555555555"
    function check(value, message) { if (!value) { failures++; console.error(message); } }
    QtObject {
        id: adapter
        property bool enabled: true
        property bool discovering: false
        property var devices: ({values: []})
    }
    ControlCentreDetails { id: details; indicators: null; bluetoothAdapter: adapter; page: "bluetooth" }
    Timer {
        interval: 50; running: true; repeat: true
        onTriggered: {
            if (Date.now() - test.started > 35000) {
                test.check(false, "Device test timed out");
                console.info("DEVICE_COMMANDS_FAILED");
                Qt.quit();
            }
            if (test.phase === 0) {
                test.check(!adapter.discovering, "Inactive page must not scan");
                details.scanForCommand(true);
                test.check(adapter.discovering, "Command starts discovery without opening the page");
                details.page = "display";
                test.check(adapter.discovering, "Navigation retains command scan");
                details.scanForCommand(false);
                test.check(!adapter.discovering, "Command releases its scan");
                adapter.discovering = true;
                details.scanForCommand(true);
                details.scanForCommand(false);
                test.check(adapter.discovering, "Command does not stop external discovery");
                adapter.discovering = false;
                details.scanForCommand(true);
                adapter.enabled = false;
                test.check(!adapter.discovering, "Adapter power-off releases scan");
                adapter.enabled = true;
                test.check(adapter.discovering, "Pending scan resumes after power-on");
                details.refreshNetwork();
                test.phase = 1;
            } else if (test.phase === 1 && !details.networkBusy && details.networkUpdatedAt > 0) {
                test.check(details.connections.length === 2 && !details.networkError, "Saved connections loaded from backend");
                test.check(details.connectionForCommand("missing", "up").ok === false, "Missing UUID rejected");
                test.check(details.connectionForCommand(test.uuid, "up").pending, "Connection request accepted");
                test.check(details.connectionForCommand(test.uuid, "down").ok === false, "Concurrent connection request rejected");
                test.phase = 2;
            } else if (test.phase === 2 && !details.networkBusy) {
                test.check(details.connections.find(item => item.uuid === test.uuid).connected, "Connection state refreshed after success");
                test.check(details.connectionForCommand(test.uuid, "up").changed === false, "Connect is idempotent");
                details.connectionForCommand(test.uuid, "down");
                test.phase = 3;
            } else if (test.phase === 3 && !details.networkBusy) {
                test.check(!details.connections.find(item => item.uuid === test.uuid).connected, "Disconnect updates state");
                details.connectionForCommand(test.failedUuid, "up");
                test.phase = 4;
            } else if (test.phase === 4 && !details.networkBusy) {
                test.check(details.connectionError.length > 0, "Failed connection reported");
                details.page = "audio";
                test.check(details.connectionError.length > 0, "Navigation preserves command failure");
                test.phase = 5;
            } else if (test.phase === 5 && !details.commandDiscovery) {
                test.check(!adapter.discovering, "Scan stops after 30-second timeout");
                console.info(test.failures ? "DEVICE_COMMANDS_FAILED" : "DEVICE_COMMANDS_PASSED");
                Qt.quit();
            }
        }
    }
}
