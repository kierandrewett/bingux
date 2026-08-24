import Quickshell
import Quickshell.Io
import QtQuick

QtObject {
    id: root

    readonly property int protocolVersion: 1
    readonly property int maxRecordBytes: 64 * 1024
    readonly property int maxQueryBytes: 512
    readonly property int minimumLimit: 1
    readonly property int maximumLimit: 50
    readonly property string socketPath: {
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/bingux/search-v1.sock" : "";
    }

    property string connectionState: "unavailable"
    property string integrationState: "unavailable"
    property int reconnectDelay: 250
    property int requestSequence: 0
    property bool rejectingConnection: false

    signal showSearch()
    signal resultsReceived(string requestId, var results, bool complete)
    signal requestFailed(string requestId, string code)
    signal activationCompleted(string requestId)

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

    function isObject(value) {
        return value !== null && typeof value === "object" && !Array.isArray(value);
    }

    function isFiniteNumber(value) {
        return typeof value === "number" && isFinite(value);
    }

    function isNonnegativeInteger(value) {
        return isFiniteNumber(value) && value >= 0 && Math.floor(value) === value;
    }

    function isValidRequestId(value) {
        return typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value);
    }

    function isValidProviderId(value) {
        return typeof value === "string" && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
    }

    function isValidQuery(query) {
        return typeof query === "string" && utf8ByteLength(query) <= maxQueryBytes;
    }

    function isValidLimit(limit) {
        return isNonnegativeInteger(limit) && limit >= minimumLimit && limit <= maximumLimit;
    }

    function isValidResult(result) {
        const kinds = ["application", "file", "folder", "database", "calculation", "weather", "chat", "action"];

        return isObject(result)
            && typeof result.resultId === "string"
            && result.resultId.length > 0
            && isValidProviderId(result.providerId)
            && kinds.indexOf(result.kind) !== -1
            && typeof result.title === "string"
            && typeof result.subtitle === "string"
            && typeof result.icon === "string"
            && isFiniteNumber(result.score)
            && result.score >= 0
            && result.score <= 1;
    }

    function isCommonRecord(record) {
        return isObject(record)
            && record.protocolVersion === protocolVersion
            && typeof record.type === "string";
    }

    function isShowSearchRecord(record) {
        return isCommonRecord(record)
            && record.type === "show-search"
            && typeof record.monotonicUsec === "string"
            && /^[0-9]+$/.test(record.monotonicUsec);
    }

    function isIntegrationStateRecord(record) {
        return isCommonRecord(record)
            && record.type === "integration-state"
            && record.name === "gnoblin-super-release"
            && (record.state === "ready" || record.state === "unavailable");
    }

    function isResultsRecord(record) {
        if (!isCommonRecord(record)
            || record.type !== "results"
            || !isValidRequestId(record.requestId)
            || typeof record.complete !== "boolean"
            || !isNonnegativeInteger(record.elapsedUsec)
            || !Array.isArray(record.results)) {
            return false;
        }

        for (let index = 0; index < record.results.length; index += 1) {
            if (!isValidResult(record.results[index])) {
                return false;
            }
        }

        return true;
    }

    function isErrorRecord(record) {
        const codes = ["invalid-request", "unsupported-protocol", "unavailable", "provider-failed", "unknown-result"];

        return isCommonRecord(record)
            && record.type === "error"
            && isValidRequestId(record.requestId)
            && codes.indexOf(record.code) !== -1
            && typeof record.message === "string";
    }

    function isActivatedRecord(record) {
        return isCommonRecord(record)
            && record.type === "activated"
            && isValidRequestId(record.requestId);
    }

    function acceptRecord(record) {
        if (isShowSearchRecord(record)) {
            root.showSearch();
            return true;
        }

        if (isIntegrationStateRecord(record)) {
            root.integrationState = record.state;
            return true;
        }

        if (isResultsRecord(record)) {
            root.resultsReceived(record.requestId, record.results, record.complete);
            return true;
        }

        if (isErrorRecord(record)) {
            root.requestFailed(record.requestId, record.code);
            return true;
        }

        if (isActivatedRecord(record)) {
            root.activationCompleted(record.requestId);
            return true;
        }

        return false;
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

    function nextRequestId(prefix) {
        requestSequence = (requestSequence + 1) % 1000000;
        return prefix + "-" + Date.now().toString(36) + "-" + requestSequence.toString(36);
    }

    function writeRecord(record) {
        if (!localSocket.connected) {
            return false;
        }

        const serialized = JSON.stringify(record);
        if (utf8ByteLength(serialized) + 1 > maxRecordBytes) {
            return false;
        }

        localSocket.write(serialized + "\n");
        localSocket.flush();
        return true;
    }

    function sendQuery(query, limit) {
        if (!isValidQuery(query) || !isValidLimit(limit)) {
            return "";
        }

        const requestId = nextRequestId("q");
        const sent = writeRecord({
            protocolVersion: protocolVersion,
            type: "query",
            requestId: requestId,
            query: query,
            limit: limit
        });

        return sent ? requestId : "";
    }

    function activate(resultId) {
        if (typeof resultId !== "string" || resultId.length === 0) {
            return "";
        }

        const requestId = nextRequestId("a");
        const sent = writeRecord({
            protocolVersion: protocolVersion,
            type: "activate",
            requestId: requestId,
            resultId: resultId
        });

        return sent ? requestId : "";
    }

    function failConnection() {
        rejectingConnection = true;
        connectionState = "unavailable";
        localSocket.connected = false;
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

    property var localSocket: Socket {
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

            root.localSocket.connected = true;
        }
    }
}
