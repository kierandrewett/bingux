import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property int protocolVersion: 2
    readonly property int maxRecordBytes: 64 * 1024
    readonly property int maxIconLength: 256
    readonly property int maxLabelLength: 2048
    readonly property int maxOutputNameLength: 128
    readonly property int maxOutputNames: 16
    readonly property int maxOutputNameBytes: 1024
    readonly property int requestTimeout: 1500
    readonly property string socketPath: {
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/bingux/osd-v2.sock" : "";
    }
    property string connectionState: "unavailable"
    property int reconnectDelay: 250
    property bool rejectingConnection: false
    property var requests: ({
    })
    property var expiryTimer
    property var osdSocket
    property var reconnectTimer

    function utf8ByteLength(value) {
        let bytes = 0;
        for (let index = 0; index < value.length; index += 1) {
            const code = value.charCodeAt(index);
            if (code < 128) {
                bytes += 1;
            } else if (code < 2048) {
                bytes += 2;
            } else if (code >= 55296 && code <= 56319 && index + 1 < value.length && value.charCodeAt(index + 1) >= 56320 && value.charCodeAt(index + 1) <= 57343) {
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

    function hasValidOutputNames(value) {
        if (!Array.isArray(value) || value.length === 0 || value.length > maxOutputNames)
            return false;

        let totalBytes = 0;
        for (let index = 0; index < value.length; index += 1) {
            const outputName = value[index];
            if (typeof outputName !== "string" || outputName.length === 0 || utf8ByteLength(outputName) > maxOutputNameLength || hasControlCharacters(outputName) || value.indexOf(outputName) !== index)
                return false;

            totalBytes += utf8ByteLength(outputName);
        }
        return totalBytes <= maxOutputNameBytes;
    }

    function isOsdRecord(record) {
        if (record === null || typeof record !== "object" || Array.isArray(record))
            return false;

        const hasValidIdentity = record.protocolVersion === protocolVersion && record.type === "osd" && isNonnegativeInteger(record.monitorIndex) && hasValidOutputNames(record.outputNames);
        if (!hasValidIdentity)
            return false;

        const hasValidText = typeof record.icon === "string" && utf8ByteLength(record.icon) <= maxIconLength && !hasControlCharacters(record.icon) && typeof record.label === "string" && utf8ByteLength(record.label) <= maxLabelLength && !hasControlCharacters(record.label);
        if (!hasValidText)
            return false;

        return isFiniteNumber(record.level) && isFiniteNumber(record.maxLevel) && record.level >= -1 && record.maxLevel >= -1;
    }

    function requestForOutputName(outputName) {
        const now = Date.now();
        for (const monitorIndex in requests) {
            const request = requests[monitorIndex];
            if (request.expiresAt > now && request.outputNames.indexOf(outputName) !== -1)
                return request;

        }
        return null;
    }

    function scheduleExpiry() {
        let earliestExpiry = 0;
        for (const monitorIndex in requests) {
            const expiry = requests[monitorIndex].expiresAt;
            if (earliestExpiry === 0 || expiry < earliestExpiry)
                earliestExpiry = expiry;

        }
        if (earliestExpiry === 0) {
            expiryTimer.stop();
            return ;
        }
        expiryTimer.interval = Math.max(1, earliestExpiry - Date.now());
        expiryTimer.restart();
    }

    function expireRequests() {
        const now = Date.now();
        const next = {
        };
        let changed = false;
        for (const monitorIndex in requests) {
            const request = requests[monitorIndex];
            if (request.expiresAt > now)
                next[monitorIndex] = request;
            else
                changed = true;
        }
        if (changed)
            requests = next;

        scheduleExpiry();
    }

    function acceptRecord(record) {
        if (!isOsdRecord(record))
            return false;

        const request = Object.assign({
        }, record, {
            "expiresAt": Date.now() + requestTimeout
        });
        const next = Object.assign({
        }, requests);
        next[String(record.monitorIndex)] = request;
        requests = next;
        scheduleExpiry();
        return true;
    }

    function ingest(recordText) {
        if (rejectingConnection)
            return ;

        if (utf8ByteLength(recordText) > maxRecordBytes) {
            failConnection();
            return ;
        }
        let record;
        try {
            record = JSON.parse(recordText);
        } catch (error) {
            failConnection();
            return ;
        }
        if (!acceptRecord(record)) {
            failConnection();
            return ;
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
        if (socketPath === "" || reconnectTimer.running)
            return ;

        reconnectTimer.interval = reconnectDelay;
        reconnectDelay = Math.min(reconnectDelay * 2, 5000);
        reconnectTimer.start();
    }

    expiryTimer: Timer {
        repeat: false
        onTriggered: root.expireRequests()
    }

    osdSocket: Socket {
        path: root.socketPath
        onConnectedChanged: {
            if (connected) {
                root.rejectingConnection = false;
                root.connectionState = "ready";
                root.reconnectDelay = 250;
                return ;
            }
            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return ;
            }
            root.connectionState = "connecting";
            root.scheduleReconnect();
        }
        onError: function(_error) {
            root.failConnection();
        }
        Component.onCompleted: {
            if (root.socketPath !== "")
                connected = true;

        }

        parser: SplitParser {
            onRead: function(data) {
                root.ingest(data);
            }
        }

    }

    reconnectTimer: Timer {
        repeat: false
        onTriggered: {
            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return ;
            }
            root.osdSocket.connected = true;
        }
    }

}
