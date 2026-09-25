import QtQuick
import Quickshell
import Quickshell.Io

pragma Singleton

// Gnoblin exposes workspace-list and workspace-switch through its compositor
// socket. The public bridge has no workspace event stream, so refresh snapshots
// at a low rate and immediately after a switch.
QtObject {
    id: root

    readonly property string socketPath: {
        const overridePath = Quickshell.env("GNOBLIN_COMPOSITOR_SOCKET");
        if (overridePath)
            return overridePath;
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/gnoblin/compositor-v1.sock" : "";
    }
    property string connectionState: "unavailable"
    property string requestState: "idle"
    property string errorMessage: ""
    property var workspaces: []
    property string requestId: ""
    property int requestSequence: 0
    property int reconnectDelay: 500
    property var socket

    readonly property bool available: connectionState === "ready" && workspaces.length > 0

    function nextRequestId() {
        requestSequence = (requestSequence + 1) % 1000000;
        return "bingux-workspaces-" + Date.now().toString(36) + "-" + requestSequence.toString(36);
    }

    function sendCommand(command, fields) {
        if (!socket?.connected || requestId !== "")
            return false;
        requestId = nextRequestId();
        requestState = command;
        requestTimer.restart();
        const record = Object.assign({ op: "command", id: requestId, command }, fields || {});
        socket.write(JSON.stringify(record) + "\n");
        socket.flush();
        return true;
    }

    function refresh() {
        if (connectionState !== "ready" || requestId !== "")
            return false;
        errorMessage = "";
        return sendCommand("workspace-list");
    }

    function switchTo(id) {
        if (connectionState !== "ready" || requestId !== "" || typeof id !== "string" || id.length === 0)
            return false;
        errorMessage = "";
        return sendCommand("workspace-switch", { workspaceId: id });
    }

    function failConnection(message) {
        errorMessage = message || "Gnoblin workspace service is unavailable";
        connectionState = "unavailable";
        requestState = "idle";
        requestId = "";
        if (socket?.connected)
            socket.connected = false;
        scheduleReconnect();
    }

    function scheduleReconnect() {
        if (socketPath === "" || reconnectTimer.running)
            return;
        reconnectTimer.interval = reconnectDelay;
        reconnectDelay = Math.min(reconnectDelay * 2, 5000);
        reconnectTimer.start();
    }

    function ingest(line) {
        let record;
        try {
            record = JSON.parse(line);
        } catch (error) {
            failConnection("Invalid response from Gnoblin workspace service");
            return;
        }
        if (!record || typeof record !== "object") {
            failConnection("Invalid response from Gnoblin workspace service");
            return;
        }
        if (record.event === "hello") {
            if (record.version !== 1) {
                failConnection("Unsupported Gnoblin compositor socket version");
                return;
            }
            connectionState = "ready";
            reconnectDelay = 500;
            refresh();
            return;
        }
        if (record.id !== requestId)
            return;

        const command = requestState;
        requestId = "";
        requestState = "idle";
        requestTimer.stop();
        if (record.event === "error") {
            errorMessage = record.message || "Workspace request failed";
            if (command === "workspace-switch")
                Qt.callLater(() => refresh());
            return;
        }
        if (record.event !== "reply" || !record.result || typeof record.result !== "object") {
            failConnection("Invalid response from Gnoblin workspace service");
            return;
        }
        if (command === "workspace-list") {
            const next = record.result.workspaces;
            if (!Array.isArray(next) || next.some(workspace =>
                !workspace || typeof workspace.id !== "string" ||
                !Number.isSafeInteger(workspace.number) || workspace.number < 1 ||
                typeof workspace.name !== "string" || typeof workspace.active !== "boolean")) {
                failConnection("Invalid workspace list from Gnoblin");
                return;
            }
            workspaces = next.slice().sort((left, right) => left.number - right.number);
            errorMessage = "";
        } else if (command === "workspace-switch") {
            const targetId = record.result.id;
            if (typeof targetId === "string")
                workspaces = workspaces.map(workspace => Object.assign({}, workspace, { active: workspace.id === targetId }));
            switchRefreshTimer.restart();
        }
    }

    socket: Socket {
        path: root.socketPath
        onConnectedChanged: {
            if (connected) {
                root.connectionState = "connecting";
                root.requestState = "idle";
                root.requestId = "";
                return;
            }
            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return;
            }
            root.connectionState = "unavailable";
            root.scheduleReconnect();
        }
        onError: {
            root.failConnection("Gnoblin workspace service is unavailable");
        }
        Component.onCompleted: if (root.socketPath !== "") connected = true
        parser: SplitParser {
            onRead: function (data) {
                root.ingest(data);
            }
        }
    }

    property var reconnectTimer: Timer {
        repeat: false
        onTriggered: if (root.socketPath !== "") root.socket.connected = true
    }

    property var requestTimer: Timer {
        interval: 3000
        repeat: false
        onTriggered: if (root.requestId !== "") root.failConnection("Gnoblin workspace request timed out");
    }

    property var switchRefreshTimer: Timer {
        interval: 250
        repeat: false
        onTriggered: root.refresh()
    }

    property var refreshTimer: Timer {
        interval: 5000
        repeat: true
        running: root.connectionState === "ready"
        onTriggered: root.refresh()
    }
}
