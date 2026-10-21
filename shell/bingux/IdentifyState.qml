import QtQuick
import Quickshell
import Quickshell.Io

// Numbered display labels requested by Settings or binguxctl. Each request
// replaces the labels and restarts the hold, so labels never stack. A change
// in the output list clears them, so a number never stays on a connector that
// was unplugged or reassigned.
Scope {
    id: root

    // The outputs labels may appear on. A test may substitute its own list.
    property var screens: Quickshell.screens
    property int holdDuration: 5000
    readonly property int maxConnectors: 16
    readonly property int maxConnectorLength: 128
    readonly property bool shown: labels.length > 0
    // [{number, connector}]. Numbers follow the order of the request, so a
    // monitor keeps its number even when another in the list has no output.
    property var labels: []

    function isConnector(value) {
        return typeof value === "string" && value.length > 0 && value.length <= maxConnectorLength && /^[A-Za-z0-9._-]+$/.test(value);
    }

    function parseConnectors(payload) {
        let connectors;
        try {
            connectors = JSON.parse(payload);
        } catch (error) {
            return null;
        }
        if (!Array.isArray(connectors) || connectors.length === 0 || connectors.length > maxConnectors)
            return null;
        const seen = ({});
        for (const connector of connectors) {
            if (!isConnector(connector) || seen[connector] === true)
                return null;
            seen[connector] = true;
        }
        return connectors;
    }

    function hasOutput(connector) {
        for (let index = 0; index < screens.length; index++) {
            if (screens[index].name === connector)
                return true;
        }
        return false;
    }

    // The number shown on an output, or 0 when that output carries no label.
    function numberFor(connector) {
        if (!shown || !hasOutput(connector))
            return 0;
        for (let index = 0; index < labels.length; index++) {
            if (labels[index].connector === connector)
                return labels[index].number;
        }
        return 0;
    }

    function show(payload) {
        const connectors = parseConnectors(payload);
        if (!connectors)
            return JSON.stringify({
                ok: false,
                error: "Identify takes 1 to " + maxConnectors + " unique connector names"
            });
        labels = connectors.map((connector, index) => ({
                    number: index + 1,
                    connector: connector
                }));
        hold.restart();
        return JSON.stringify({
            ok: true,
            labelled: connectors.filter(connector => hasOutput(connector))
        });
    }

    function hide() {
        hold.stop();
        labels = [];
        return JSON.stringify({
            ok: true
        });
    }

    onScreensChanged: hide()

    IpcHandler {
        target: "identify"
        function show(payload: string): string {
            return root.show(payload);
        }
        function hide(): string {
            return root.hide();
        }
    }

    property var hold: Timer {
        interval: root.holdDuration
        repeat: false
        onTriggered: root.hide()
    }
}
