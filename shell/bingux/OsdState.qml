import Quickshell
import Quickshell.Io
import QtQuick

QtObject {
    id: root

    readonly property int protocolVersion: 1
    readonly property int maxRecordBytes: 64 * 1024
    readonly property int maxIconLength: 256
    readonly property int maxLabelLength: 2048
    readonly property string socketPath: {
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/bingux/osd-v1.sock" : "";
    }

    property string connectionState: "unavailable"
    property int reconnectDelay: 250
    property bool rejectingConnection: false
    property var requests: ({})

    function utf8ByteLength(value) {
        let bytes = 0;

        for (let index = 0; index < value.length; index += 1) {
            const code = value.charCodeAt(index);

            if (code < 0x80) {
                bytes += 1;
            } else if (code < 0x800) {
                bytes += 2;
            } else if (code >= 0xd800 && code <= 0xdbff
                       && index + 1 < value.length
                       && value.charCodeAt(index + 1) >= 0xdc00
                       && value.charCodeAt(index + 1) <= 0xdfff) {
                bytes += 4;
                index += 1;
            } else {
                bytes += 3;
            }
        }

        return bytes;
    }


    function isFiniteNumber(value) {
        return typeof value === "number" && isFinite(value);
    }

    function isNonnegativeInteger(value) {
        return isFiniteNumber(value) && value >= 0 && Math.floor(value) === value;
    }

    function hasControlCharacters(value) {
        return /[\u0000-\u001f\u007f]/.test(value);
    }

    function isOsdRecord(record) {
        return record !== null
            && typeof record === "object"
            && !Array.isArray(record)
            && record.protocolVersion === protocolVersion
            && record.type === "osd"
            && isNonnegativeInteger(record.monitorIndex)
            && typeof record.icon === "string"
            && utf8ByteLength(record.icon) <= maxIconLength
            && !hasControlCharacters(record.icon)
            && typeof record.label === "string"
            && utf8ByteLength(record.label) <= maxLabelLength
            && !hasControlCharacters(record.label)
            && isFiniteNumber(record.level)
            && isFiniteNumber(record.maxLevel)
            && record.level >= -1
            && record.maxLevel >= -1;
    }

    function requestForMonitor(monitorIndex) {
        return requests[String(monitorIndex)] || null;
    }

    function acceptRecord(record) {
        if (!isOsdRecord(record)) {
            return false;
        }

        const next = Object.assign({}, requests);
        next[String(record.monitorIndex)] = record;
        requests = next;
        return true;
    }

    function clearRequest(monitorIndex, request) {
        const key = String(monitorIndex);

        if (requests[key] !== request) {
            return;
        }

        const next = Object.assign({}, requests);
        delete next[key];
        requests = next;
    }

    function ingest(recordText) {
        if (rejectingConnection) {
            return;
        }

        if (utf8ByteLength(recordText) > maxRecordBytes) {
            failConnection();
            return;
        }

        let record;
        try {
            record = JSON.parse(recordText);
        } catch (error) {
            failConnection();
            return;
        }

        if (!acceptRecord(record)) {
            failConnection();
            return;
        }

        reconnectDelay = 250;
    }

    function failConnection() {
        rejectingConnection = true;
        connectionState = "unavailable";
        osdSocket.connected = false;
        scheduleReconnect();
    }

    function scheduleReconnect() {
        if (socketPath === "" || reconnectTimer.running) {
            return;
        }

        reconnectTimer.interval = reconnectDelay;
        reconnectDelay = Math.min(reconnectDelay * 2, 5000);
        reconnectTimer.start();
    }

    property var osdSocket: Socket {
        path: root.socketPath
        parser: SplitParser {
            onRead: function(data) {
                root.ingest(data);
            }
        }

        onConnectedChanged: {
            if (connected) {
                root.rejectingConnection = false;
                root.connectionState = "ready";
                root.reconnectDelay = 250;
                return;
            }

            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return;
            }

            root.connectionState = "connecting";
            root.scheduleReconnect();
        }

        onError: function(_error) {
            root.failConnection();
        }

        Component.onCompleted: {
            if (root.socketPath !== "") {
                connected = true;
            }
        }
    }

    property var reconnectTimer: Timer {
        repeat: false

        onTriggered: {
            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return;
            }

            root.osdSocket.connected = true;
        }
    }
}
