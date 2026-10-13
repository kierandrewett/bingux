import Quickshell
import Quickshell.Io
import QtQuick
import "MetricsHistory.js" as History

QtObject {
    id: root

    property var latest: null
    property var history: []
    property double lastUpdatedAt: 0
    property var sampleSnapshot: null
    property double connectionStartedAt: 0
    property double now: Date.now()
    property string connectionState: "unavailable"
    property int reconnectDelay: 250
    readonly property int maxInputSources: 32
    readonly property int maxInputSourceFieldLength: 128
    readonly property bool available: latest !== null && now - lastUpdatedAt <= 5000
    readonly property string socketPath: {
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/bingux/metrics-v1.sock" : "";
    }
    readonly property string cpuLabel: available && latest.cpuPercent !== null ? "CPU " + Math.round(latest.cpuPercent) + "%" : "CPU --"
    readonly property string memoryLabel: available ? "MEM " + formatBytes(latest.memoryUsedBytes) + "/" + formatBytes(latest.memoryTotalBytes) : "MEM --"
    readonly property string networkLabel: available ? "NET RX " + formatRate(latest.networkReceiveBytesPerSecond) + " TX " + formatRate(latest.networkTransmitBytesPerSecond) : "NET --"

    readonly property bool desktopStateAvailable: available && latest.desktopStateAvailable
    readonly property var inputSources: desktopStateAvailable ? latest.inputSources : []
    readonly property var currentInputSource: desktopStateAvailable ? latest.currentInputSource : null
    readonly property bool privacyAvailable: available && (latest.privacyAvailable === undefined ? typeof latest.screenSharing === "boolean" || typeof latest.microphoneInUse === "boolean" || typeof latest.locationInUse === "boolean" : latest.privacyAvailable)
    readonly property bool screenSharingAvailable: privacyAvailable && (latest.screenSharingAvailable === undefined || latest.screenSharingAvailable)
    readonly property bool microphoneAvailable: privacyAvailable && (latest.microphoneAvailable === undefined || latest.microphoneAvailable)
    readonly property bool locationAvailable: privacyAvailable && (latest.locationAvailable === undefined || latest.locationAvailable)
    readonly property bool screenSharing: screenSharingAvailable && latest.screenSharing
    readonly property bool microphoneInUse: microphoneAvailable && latest.microphoneInUse
    readonly property bool microphoneCaptureFeedPresent: available && typeof latest.microphoneCapturesAvailable === "boolean"
    readonly property bool microphoneCapturesAvailable: available && latest.microphoneCapturesAvailable === true
    readonly property var microphoneCaptures: microphoneCapturesAvailable ? latest.microphoneCaptures : []
    readonly property bool locationInUse: locationAvailable && latest.locationInUse
    readonly property string inputSourceLabel: {
        const source = currentInputSource;

        if (source === null) {
            return "";
        }

        const label = source.shortName !== "" ? source.shortName : source.id;
        return label.slice(0, 32);
    }

    function formatBytes(bytes) {
        const units = ["B", "K", "M", "G", "T"];
        let unitIndex = 0;
        let value = bytes;

        while (value >= 1024 && unitIndex < units.length - 1) {
            value /= 1024;
            unitIndex += 1;
        }

        return (value >= 10 || unitIndex === 0 ? Math.round(value) : value.toFixed(1)) + units[unitIndex];
    }

    function formatRate(bytesPerSecond) {
        return bytesPerSecond === null ? "--" : formatBytes(bytesPerSecond) + "/s";
    }

    function isFiniteNumber(value) {
        return typeof value === "number" && isFinite(value);
    }

    function isOptionalNumber(value) {
        return value === null || isFiniteNumber(value);
    }

    function isInputSource(source) {
        return source !== null && typeof source === "object" && typeof source.type === "string" && typeof source.id === "string" && typeof source.shortName === "string" && typeof source.displayName === "string" && source.type.length <= root.maxInputSourceFieldLength && source.id.length <= root.maxInputSourceFieldLength && source.shortName.length <= root.maxInputSourceFieldLength && source.displayName.length <= root.maxInputSourceFieldLength;
    }

    function isInputSourceList(sources) {
        if (!Array.isArray(sources) || sources.length > root.maxInputSources) {
            return false;
        }

        for (let index = 0; index < sources.length; index += 1) {
            if (!isInputSource(sources[index])) {
                return false;
            }
        }

        return true;
    }

    function isOptionalBoolean(value) {
        return typeof value === "undefined" || typeof value === "boolean";
    }

    function isDesktopStateRecord(record) {
        if (typeof record.desktopStateAvailable === "undefined") {
            return true;
        }

        return typeof record.desktopStateAvailable === "boolean" && isInputSourceList(record.inputSources) && (record.currentInputSource === null || isInputSource(record.currentInputSource)) && typeof record.screenSharing === "boolean" && typeof record.microphoneInUse === "boolean" && typeof record.locationInUse === "boolean" && isOptionalBoolean(record.privacyAvailable) && isOptionalBoolean(record.screenSharingAvailable) && isOptionalBoolean(record.microphoneAvailable) && isOptionalBoolean(record.locationAvailable);
    }

    function isHardwareRecord(hardware) {
        if (hardware === undefined)
            return true;
        if (!hardware || typeof hardware !== "object")
            return false;
        const string = value => typeof value === "string" && value.length <= 4096;
        const number = value => isFiniteNumber(value) && value >= 0;
        const optional = value => value === null || number(value);
        return string(hardware.cpuModel) && string(hardware.kernel) && optional(hardware.cpuMhz) && optional(hardware.uptimeSeconds) && [hardware.processCount, hardware.runningProcesses, hardware.threads].every(number) && Array.isArray(hardware.gpus) && hardware.gpus.length <= 8 && hardware.gpus.every(gpu => gpu && [gpu.name, gpu.driver, gpu.pciAddress].every(string) && [gpu.busyPercent, gpu.memoryUsedBytes, gpu.memoryTotalBytes, gpu.temperatureCelsius, gpu.powerWatts, gpu.clockMhz, gpu.fanRpm].every(optional)) && Array.isArray(hardware.storage) && hardware.storage.length <= 2 && hardware.storage.every(volume => volume && string(volume.path) && number(volume.totalBytes) && number(volume.availableBytes)) && Array.isArray(hardware.processes) && hardware.processes.length <= 8192 && hardware.processes.every(process => process && string(process.name) && string(process.state) && (process.executable === undefined || string(process.executable)) && (process.startTime === undefined || number(process.startTime)) && [process.pid, process.memoryBytes, process.threads].every(number) && optional(process.cpuPercent));
    }

    function isExtraRecord(extra) {
        if (extra === undefined)
            return true;
        if (extra === null || typeof extra !== "object" || Array.isArray(extra))
            return false;
        return (extra.cpuCores === undefined || (Array.isArray(extra.cpuCores) && extra.cpuCores.length <= 256 && extra.cpuCores.every(core => core && Number.isInteger(core.id) && core.id >= 0 && (core.usage === null || (isFiniteNumber(core.usage) && core.usage >= 0 && core.usage <= 100))) && new Set(extra.cpuCores.map(core => core.id)).size === extra.cpuCores.length)) && isHardwareRecord(extra.hardware) && ["cpuTemperatureCelsius", "load1", "load5", "load15", "logicalCpus", "swapUsedBytes", "swapTotalBytes", "diskReadBytesPerSecond", "diskWriteBytesPerSecond"].every(key => extra[key] === undefined || extra[key] === null || (isFiniteNumber(extra[key]) && extra[key] >= 0));
    }

    function isMetricsRecord(record) {
        return record !== null && typeof record === "object" && record.protocolVersion === 1 && record.type === "metrics" && isOptionalNumber(record.cpuPercent) && isFiniteNumber(record.memoryTotalBytes) && isFiniteNumber(record.memoryUsedBytes) && isOptionalNumber(record.networkReceiveBytesPerSecond) && isOptionalNumber(record.networkTransmitBytesPerSecond) && record.memoryTotalBytes >= 0 && record.memoryUsedBytes >= 0 && record.memoryUsedBytes <= record.memoryTotalBytes && (record.cpuPercent === null || (record.cpuPercent >= 0 && record.cpuPercent <= 100)) && isDesktopStateRecord(record) && isMicrophoneCaptureRecord(record) && isExtraRecord(record.extra);
    }

    function isMicrophoneCaptureRecord(record) {
        if (record.microphoneCapturesAvailable === undefined && record.microphoneCaptures === undefined)
            return true;
        const field = value => typeof value === "string" && value.length <= 128 && !/[\u0000-\u001f\u007f-\u009f]/.test(value);
        return typeof record.microphoneCapturesAvailable === "boolean" && Array.isArray(record.microphoneCaptures) && record.microphoneCaptures.length <= 32 && record.microphoneCaptures.every(capture => capture !== null && typeof capture === "object" && field(capture.app) && field(capture.appId) && field(capture.device) && typeof capture.muted === "boolean");
    }

    function ingest(recordText) {
        if (recordText.length > 8 * 1024 * 1024) {
            failConnection("record is larger than the metrics protocol limit");
            return;
        }

        let record;
        try {
            record = JSON.parse(recordText);
        } catch (error) {
            failConnection("record is not valid JSON");
            return;
        }

        if (!isMetricsRecord(record)) {
            failConnection("record does not match the metrics-v1 contract");
            return;
        }

        const freshSample = record.extra?.sampledAtMs === undefined || record.extra.sampledAtMs !== sampleSnapshot?.extra?.sampledAtMs;
        latest = record;
        if (freshSample) {
            sampleSnapshot = record;
            lastUpdatedAt = Date.now();
            history = History.append(history, record, lastUpdatedAt);
            now = lastUpdatedAt;
            armFreshnessTimeout(lastUpdatedAt);
        }
        connectionState = "ready";
        reconnectDelay = 250;
    }

    function failConnection(reason) {
        console.warn("[bingux-metrics] " + reason);
        connectionState = "unavailable";
        dropSocket();
        scheduleReconnect();
    }

    function scheduleReconnect() {
        // A failed connection can signal from inside the one-shot timer.
        // Arm after that callback finishes so its running flag cannot hide
        // the next retry.
        Qt.callLater(root.armReconnect);
    }

    function armReconnect() {
        if (socketPath === "") {
            connectionState = "unavailable";
            return;
        }

        if ((!root.metricsSocket || !root.metricsSocket.connected) && !root.reconnectTimer.running) {
            root.reconnectTimer.interval = reconnectDelay;
            reconnectDelay = Math.min(reconnectDelay * 2, 5000);
            root.reconnectTimer.start();
        }
    }

    function armFreshnessTimeout(referenceAt) {
        root.freshnessTimer.interval = Math.max(1, Math.ceil(5000 - (Date.now() - referenceAt)));
        root.freshnessTimer.restart();
    }

    property var metricsSocket: null
    function dropSocket() {
        const socket = metricsSocket;
        metricsSocket = null;
        if (socket)
            socket.destroy();
    }
    function connectSocket() {
        if (socketPath === "")
            return;
        dropSocket();
        metricsSocket = socketComponent.createObject(root);
        metricsSocket.connected = true;
    }
    Component.onCompleted: connectSocket()
    property Component socketComponent: Component {
        Socket {
            id: connection
            path: root.socketPath
            parser: SplitParser {
                onRead: function (data) {
                    if (root.metricsSocket === connection)
                        root.ingest(data);
                }
            }

            onConnectedChanged: {
                if (root.metricsSocket !== connection)
                    return;
                if (connected) {
                    root.connectionStartedAt = Date.now();
                    root.armFreshnessTimeout(root.connectionStartedAt);
                    root.connectionState = "ready";
                    root.reconnectDelay = 250;
                    return;
                }

                root.connectionStartedAt = 0;
                root.connectionState = "connecting";
                root.dropSocket();
                root.scheduleReconnect();
            }

            onError: function (error) {
                if (root.metricsSocket === connection)
                    root.failConnection(error);
            }
        }
    }

    property var freshnessTimer: Timer {
        repeat: false

        onTriggered: {
            root.now = Date.now();
            if (root.metricsSocket && root.metricsSocket.connected) {
                root.failConnection("metrics record is stale");
            }
        }
    }

    property var reconnectTimer: Timer {
        onTriggered: {
            if (root.socketPath === "") {
                root.connectionState = "unavailable";
                return;
            }

            root.connectSocket();
        }
    }
}
