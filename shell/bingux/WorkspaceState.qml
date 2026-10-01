pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Workspace reads and activation use the versioned compositor API. The windows
// subscription also carries workspace lifecycle events, so polling is only
// needed for older API versions.
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
    property int apiMajor: 0
    property int apiMinor: 0
    property bool workspaceEventsAvailable: false
    property bool refreshQueued: false
    property int requestSequence: 0
    property int reconnectDelay: 500
    property var socket

    readonly property bool available: connectionState === "ready" && workspaces.length > 0

    function nextRequestId() {
        requestSequence = (requestSequence + 1) % 1000000;
        return "bingux-workspaces-" + Date.now().toString(36) + "-" + requestSequence.toString(36);
    }

    function sendApi(method, arguments_) {
        if (!socket?.connected || requestId !== "")
            return false;
        requestId = nextRequestId();
        requestState = method;
        requestTimer.restart();
        const record = {
            op: "api",
            api_version: {
                major: apiMajor,
                minor: apiMinor
            },
            id: requestId,
            method,
            arguments: arguments_ || {}
        };
        socket.write(JSON.stringify(record) + "\n");
        socket.flush();
        return true;
    }

    function refresh() {
        if (connectionState !== "ready" || requestId !== "")
            return false;
        errorMessage = "";
        return sendApi("workspace.list", {});
    }

    function switchTo(id) {
        if (connectionState !== "ready" || requestId !== "" || typeof id !== "string" || id.length === 0)
            return false;
        errorMessage = "";
        return sendApi("workspace.switch", {
            id
        });
    }

    function failConnection(message) {
        errorMessage = message || "Gnoblin workspace service is unavailable";
        connectionState = "unavailable";
        requestState = "idle";
        requestId = "";
        apiMajor = 0;
        apiMinor = 0;
        workspaceEventsAvailable = false;
        refreshQueued = false;
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
            if (record.version !== 1 || record.api_major !== 1 || !Number.isSafeInteger(record.api_minor) || record.api_minor < 0 || !Array.isArray(record.methods) || !record.methods.includes("workspace.list") || !record.methods.includes("workspace.switch")) {
                failConnection("Unsupported Gnoblin compositor socket version");
                return;
            }
            apiMajor = record.api_major;
            apiMinor = record.api_minor;
            workspaceEventsAvailable = false;
            connectionState = "ready";
            reconnectDelay = 500;
            if (apiMinor >= 1) {
                const subscription = {
                    op: "windows",
                    api_version: {
                        major: 1,
                        minor: 1
                    }
                };
                socket.write(JSON.stringify(subscription) + "\n");
                socket.flush();
            }
            refresh();
            return;
        }
        if (record.event === "windows") {
            workspaceEventsAvailable = true;
            return;
        }
        if (typeof record.event === "string" && record.event.startsWith("gnoblin.workspace.")) {
            workspaceEventsAvailable = true;
            if (requestId !== "")
                refreshQueued = true;
            else
                switchRefreshTimer.restart();
            return;
        }
        if (record.event === "error" && !record.id) {
            workspaceEventsAvailable = false;
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
            if (command === "workspace.switch" || refreshQueued) {
                refreshQueued = false;
                Qt.callLater(() => refresh());
            }
            return;
        }
        if (record.event !== "reply" || !record.result || typeof record.result !== "object") {
            failConnection("Invalid response from Gnoblin workspace service");
            return;
        }
        if (command === "workspace.list") {
            const next = record.result.workspaces;
            if (!Array.isArray(next) || next.some(workspace => !workspace || typeof workspace.id !== "string" || !Number.isSafeInteger(workspace.number) || workspace.number < 1 || typeof workspace.name !== "string" || typeof workspace.active !== "boolean")) {
                failConnection("Invalid workspace list from Gnoblin");
                return;
            }
            workspaces = next.slice().sort((left, right) => left.number - right.number);
            errorMessage = "";
        } else if (command === "workspace.switch") {
            switchRefreshTimer.restart();
        }
        if (refreshQueued) {
            refreshQueued = false;
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
                root.apiMajor = 0;
                root.apiMinor = 0;
                root.workspaceEventsAvailable = false;
                root.refreshQueued = false;
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
        Component.onCompleted: if (root.socketPath !== "")
            connected = true
        parser: SplitParser {
            onRead: function (data) {
                root.ingest(data);
            }
        }
    }

    property var reconnectTimer: Timer {
        repeat: false
        onTriggered: if (root.socketPath !== "")
            root.socket.connected = true
    }

    property var requestTimer: Timer {
        interval: 3000
        repeat: false
        onTriggered: if (root.requestId !== "")
            root.failConnection("Gnoblin workspace request timed out")
    }

    property var switchRefreshTimer: Timer {
        interval: 250
        repeat: false
        onTriggered: root.refresh()
    }

    property var refreshTimer: Timer {
        interval: 5000
        repeat: true
        running: root.connectionState === "ready" && !root.workspaceEventsAvailable
        onTriggered: root.refresh()
    }
}
