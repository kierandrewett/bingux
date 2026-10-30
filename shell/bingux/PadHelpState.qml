import QtQuick
import Quickshell
import Quickshell.Io

// Tablet-pad help overlay requests from bingux-statusd. Only one request is
// held: a new request replaces the current one and restarts the timeout, so
// overlays never stack. A pad-help-clear record, a dropped connection, a
// malformed record or the timeout all clear it (fail closed).
QtObject {
    id: root

    readonly property int protocolVersion: 1
    readonly property int maxRecordBytes: 64 * 1024
    readonly property int maxDeviceIdBytes: 128
    readonly property int maxDeviceNameBytes: 128
    readonly property int maxDeviceTypeBytes: 64
    readonly property int maxLabelBytes: 256
    readonly property int maxLabelBytesTotal: 8 * 1024
    readonly property int maxOutputNameBytes: 128
    readonly property int maxOutputNames: 16
    readonly property int maxOutputNameBytesTotal: 1024
    readonly property int maxButtons: 64
    readonly property int maxModeGroups: 8
    readonly property int maxSwitchButtons: 64
    readonly property int maxFeatures: 32
    // Auto-hide delay for a request, in milliseconds.
    property int requestTimeout: 10000
    readonly property string socketPath: {
        const runtimeDirectory = Quickshell.env("XDG_RUNTIME_DIR");
        return runtimeDirectory ? runtimeDirectory + "/bingux/pad-help-v1.sock" : "";
    }

    // The current request, or null when no overlay should be shown.
    property var request: null
    property int reconnectDelay: 250
    property bool rejectingConnection: false
    property var expiryTimer
    property var padSocket
    property var reconnectTimer

    // The connector the overlay is drawn on: the first name in the request
    // that is a connected output. Empty when none is connected, so a request
    // for an absent output never appears on some other screen.
    readonly property var screens: Quickshell.screens
    readonly property string targetOutputName: {
        if (request === null)
            return "";
        for (const outputName of request.outputNames) {
            if (hasOutput(outputName))
                return outputName;
        }
        return "";
    }

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

    function hasOutput(outputName) {
        for (let index = 0; index < screens.length; index += 1) {
            if (screens[index].name === outputName)
                return true;
        }
        return false;
    }

    function isNonnegativeInteger(value) {
        return typeof value === "number" && isFinite(value) && value >= 0 && Math.floor(value) === value;
    }

    // Matches the daemon's control-character rule (C0, DEL and C1).
    function hasControlCharacters(value) {
        return /[\u0000-\u001f\u007f-\u009f]/.test(value);
    }

    // Text may be empty unless the caller requires otherwise.
    function isText(value, maxBytes) {
        return typeof value === "string" && utf8ByteLength(value) <= maxBytes && !hasControlCharacters(value);
    }

    // A label may be absent; a present one must be bounded text.
    function isOptionalLabel(value) {
        return value === undefined || isText(value, maxLabelBytes);
    }

    function hasValidOutputNames(value) {
        if (!Array.isArray(value) || value.length === 0 || value.length > maxOutputNames)
            return false;

        let totalBytes = 0;
        for (let index = 0; index < value.length; index += 1) {
            const outputName = value[index];
            if (!isText(outputName, maxOutputNameBytes) || outputName.length === 0 || value.indexOf(outputName) !== index)
                return false;

            totalBytes += utf8ByteLength(outputName);
        }
        return totalBytes <= maxOutputNameBytesTotal;
    }

    function hasValidDevice(device) {
        return device !== null && typeof device === "object" && !Array.isArray(device) && isText(device.id, maxDeviceIdBytes) && device.id.length > 0 && isText(device.name, maxDeviceNameBytes) && isText(device.deviceType, maxDeviceTypeBytes);
    }

    function hasUniqueKeys(items, keyOf) {
        const seen = ({});
        for (const item of items) {
            const key = keyOf(item);
            if (seen[key] === true)
                return false;
            seen[key] = true;
        }
        return true;
    }

    function isValidButton(button) {
        return button !== null && typeof button === "object" && isNonnegativeInteger(button.button) && isOptionalLabel(button.label) && (button.modeGroup === undefined || isNonnegativeInteger(button.modeGroup));
    }

    function isValidModeGroup(group) {
        return group !== null && typeof group === "object" && isNonnegativeInteger(group.group) && isNonnegativeInteger(group.modeCount) && Array.isArray(group.switchButtons) && group.switchButtons.length <= maxSwitchButtons && group.switchButtons.every(isNonnegativeInteger);
    }

    function isValidFeature(feature) {
        if (feature === null || typeof feature !== "object" || !isNonnegativeInteger(feature.index) || !isOptionalLabel(feature.positiveLabel) || !isOptionalLabel(feature.negativeLabel))
            return false;
        return feature.type === "ring" || feature.type === "dial" || feature.type === "strip";
    }

    function totalLabelBytes(record) {
        let total = 0;
        for (const button of record.buttons) {
            if (button.label !== undefined)
                total += utf8ByteLength(button.label);
        }
        for (const feature of record.features) {
            if (feature.positiveLabel !== undefined)
                total += utf8ByteLength(feature.positiveLabel);
            if (feature.negativeLabel !== undefined)
                total += utf8ByteLength(feature.negativeLabel);
        }
        return total;
    }

    function isPadHelpRecord(record) {
        if (record === null || typeof record !== "object" || Array.isArray(record))
            return false;
        if (record.protocolVersion !== protocolVersion || record.type !== "pad-help")
            return false;
        if (!isNonnegativeInteger(record.monitorIndex) || !hasValidOutputNames(record.outputNames))
            return false;
        if (typeof record.editionMode !== "boolean" || !hasValidDevice(record.device))
            return false;
        if (!Array.isArray(record.buttons) || record.buttons.length > maxButtons || !record.buttons.every(isValidButton) || !hasUniqueKeys(record.buttons, button => button.button))
            return false;
        if (!Array.isArray(record.modeGroups) || record.modeGroups.length > maxModeGroups || !record.modeGroups.every(isValidModeGroup) || !hasUniqueKeys(record.modeGroups, group => group.group))
            return false;
        if (!Array.isArray(record.features) || record.features.length > maxFeatures || !record.features.every(isValidFeature) || !hasUniqueKeys(record.features, feature => feature.type + ":" + feature.index))
            return false;
        return totalLabelBytes(record) <= maxLabelBytesTotal;
    }

    function isClearRecord(record) {
        return record !== null && typeof record === "object" && !Array.isArray(record) && record.protocolVersion === protocolVersion && record.type === "pad-help-clear";
    }

    // Shows the request, replacing any current one. The timeout restarts.
    function show(record) {
        request = record;
        expiryTimer.restart();
    }

    // Hides the overlay at once. Dismiss, Escape, clear and the timeout all
    // come through here.
    function dismiss() {
        expiryTimer.stop();
        request = null;
    }

    function acceptRecord(record) {
        if (isClearRecord(record)) {
            dismiss();
            return true;
        }
        if (!isPadHelpRecord(record))
            return false;
        show(record);
        return true;
    }

    // Returns false for a malformed record. The connection is dropped, which
    // also clears any overlay, so a broken bridge never leaves one on screen.
    function ingest(recordText) {
        if (rejectingConnection)
            return false;

        if (utf8ByteLength(recordText) > maxRecordBytes) {
            failConnection();
            return false;
        }
        let record;
        try {
            record = JSON.parse(recordText);
        } catch (error) {
            failConnection();
            return false;
        }
        if (!acceptRecord(record)) {
            failConnection();
            return false;
        }
        reconnectDelay = 250;
        return true;
    }

    function failConnection() {
        rejectingConnection = true;
        dismiss();
        padSocket.connected = false;
        scheduleReconnect();
    }

    function scheduleReconnect() {
        if (socketPath === "" || reconnectTimer.running)
            return;

        reconnectTimer.interval = reconnectDelay;
        reconnectDelay = Math.min(reconnectDelay * 2, 5000);
        reconnectTimer.start();
    }

    expiryTimer: Timer {
        interval: root.requestTimeout
        repeat: false
        onTriggered: root.dismiss()
    }

    padSocket: Socket {
        path: root.socketPath
        onConnectedChanged: {
            if (connected) {
                root.rejectingConnection = false;
                root.reconnectDelay = 250;
                return;
            }
            // A dropped bridge must not leave its last overlay behind.
            root.dismiss();
            root.scheduleReconnect();
        }
        onError: function (_error) {
            root.failConnection();
        }
        Component.onCompleted: {
            if (root.socketPath !== "")
                connected = true;
        }

        parser: SplitParser {
            onRead: function (data) {
                root.ingest(data);
            }
        }
    }

    reconnectTimer: Timer {
        repeat: false
        onTriggered: {
            if (root.socketPath !== "")
                root.padSocket.connected = true;
        }
    }
}
