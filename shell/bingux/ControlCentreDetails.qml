import QtQuick
import QtQuick.Effects
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Io
import Quickshell.Services.Pipewire
import "ControlCentreModel.js" as DetailModel

Item {
    id: root
    implicitHeight: detailLayout.implicitHeight
    required property var indicators
    property var bluetoothAdapter: null
    property string page: "network"
    property bool active: false
    property var connections: []
    property string audioTab: "output"
    readonly property var networkSections: DetailModel.networkSections(connections, wirelessNetworks)
    property string errorText: ""
    property string pendingConnection: ""
    property string networkAction: "up"
    property var wirelessNetworks: []
    property bool commandDiscovery: false
    property string networkError: ""
    property string connectionError: ""
    property double networkUpdatedAt: 0
    readonly property bool networkBusy: networkQuery.running || wifiQuery.running || connectNetwork.running
    function scanForCommand(enabled) {
        commandDiscovery = enabled;
        if (enabled) scanTimeout.restart(); else { scanTimeout.stop(); syncDiscovery(); }
    }
    Timer { id: scanTimeout; interval: 30000; onTriggered: root.commandDiscovery = false }
    function connectionForCommand(identity, action) {
        if (!["up", "down"].includes(action)) return {ok: false, error: "Unknown connection action"};
        if (connectNetwork.running) return {ok: false, error: "A connection change is in progress"};
        const connection = connections.find(item => item.uuid === identity);
        if (!connection) return {ok: false, error: "Saved connection not found; refresh the network list"};
        if (connection.connected === (action === "up")) return {ok: true, changed: false};
        networkAction = action;
        connectionError = "";
        errorText = "";
        pendingConnection = identity;
        connectNetwork.running = true;
        return {ok: true, pending: true};
    }
    property var discoveryAdapter: null
    readonly property bool discoveryEligible: (commandDiscovery || active && page === "bluetooth") && !!bluetoothAdapter && bluetoothAdapter.enabled
    readonly property bool discoveryRunning: !!discoveryAdapter || (!!bluetoothAdapter && bluetoothAdapter.discovering)
    onDiscoveryEligibleChanged: syncDiscovery()
    onBluetoothAdapterChanged: syncDiscovery()
    function syncDiscovery() {
        if (discoveryAdapter && (!discoveryEligible || discoveryAdapter !== bluetoothAdapter)) stopDiscovery();
        if (discoveryEligible) startDiscovery();
    }
    function startDiscovery() {
        if (!discoveryEligible || discoveryAdapter || bluetoothAdapter.discovering) return;
        discoveryAdapter = bluetoothAdapter;
        discoveryAdapter.discovering = true;
    }
    property var audioInputs: Pipewire.nodes.values.filter(node => !node.isSink && !node.isStream && node.audio !== null)
    property var inputNode: Pipewire.defaultAudioSource
    property var outputNode: Pipewire.defaultAudioSink
    signal inputSelected(var node)
    signal outputSelected(var node)
    onInputSelected: node => Pipewire.preferredDefaultAudioSource = node
    onOutputSelected: node => Pipewire.preferredDefaultAudioSink = node
    function stopDiscovery() {
        if (discoveryAdapter) discoveryAdapter.discovering = false;
        discoveryAdapter = null;
    }
    function toggleDiscovery() {
        if (!discoveryEligible) return;
        if (discoveryAdapter) stopDiscovery();
        else startDiscovery();
    }
    Component.onDestruction: stopDiscovery()
    function splitNetworkLine(line) {
        const fields = []; let field = ""; let escaped = false;
        for (const character of line) {
            if (escaped) { field += character; escaped = false; }
            else if (character === "\\") escaped = true;
            else if (character === ":") { fields.push(field); field = ""; }
            else field += character;
        }
        fields.push(field); return fields;
    }
    property var audioOutputs: Pipewire.nodes.values.filter(node => node.isSink && !node.isStream)
    readonly property var bluetoothDevices: bluetoothAdapter && bluetoothAdapter.devices ? bluetoothAdapter.devices.values.filter(device => device.paired || device.connected).sort((a, b) => Number(b.connected) - Number(a.connected)) : []
    readonly property var nearbyDevices: bluetoothAdapter && bluetoothAdapter.devices ? bluetoothAdapter.devices.values.filter(device => !device.paired && !device.connected) : []
    readonly property bool keyboardNavigation: back.visualFocus
    function focusBack(reason) { back.forceActiveFocus(reason); }
    Keys.onLeftPressed: root.backRequested()
    Keys.onEscapePressed: root.backRequested()
    signal backRequested()
    signal settingsRequested(string panel)
    onActiveChanged: { if (active && page === "network") refreshNetwork(); if (!active && !commandDiscovery) stopDiscovery(); }
    onAudioTabChanged: { deviceList.cancelFlick(); deviceList.contentY = 0; }
    onPageChanged: { deviceList.cancelFlick(); deviceList.contentY = 0; if (page !== "bluetooth" && !commandDiscovery) stopDiscovery(); errorText = ""; if (active && page === "network") refreshNetwork(); }
    function refreshNetwork() { if (!networkQuery.running && !connectNetwork.running) { errorText = ""; networkError = ""; networkQuery.running = true; } if (!wifiQuery.running) wifiQuery.running = true; }
    Timer { interval: 10000; repeat: true; running: root.active && root.page === "network"; onTriggered: root.refreshNetwork() }
    Process {
        id: networkQuery
        command: ["nmcli", "--terse", "--escape", "no", "--fields", "UUID,TYPE,NAME,DEVICE", "connection", "show"]
        stdout: StdioCollector {
            onStreamFinished: {
                const rows = [];
                for (const line of text.trim().split("\n")) {
                    const fields = line.split(":");
                    if (fields.length < 4 || !["802-11-wireless", "802-3-ethernet", "vpn", "wireguard", "tun", "gsm", "cdma", "bluetooth"].includes(fields[1])) continue;
                    rows.push({uuid: fields[0], type: fields[1], name: fields.slice(2, -1).join(":"), connected: fields[fields.length - 1] !== "" && fields[fields.length - 1] !== "--"});
                }
                const sorted = rows.sort((a, b) => Number(b.connected) - Number(a.connected) || a.name.localeCompare(b.name));
                if (JSON.stringify(sorted) !== JSON.stringify(root.connections)) root.connections = sorted;
            }
        }
        onExited: exitCode => {
            root.networkUpdatedAt = Date.now();
            root.networkError = exitCode === 0 ? "" : "Could not load connections. Open Network settings to manage them.";
            if (root.networkError) root.errorText = root.networkError;
        }
    }
    Process {
        id: connectNetwork
        command: ["nmcli", "--wait", "10", "connection", root.networkAction, "uuid", root.pendingConnection]
        onExited: exitCode => {
            root.pendingConnection = "";
            root.refreshNetwork();
            root.connectionError = exitCode === 0 ? "" : "Could not change this connection. Open Network settings to check it.";
            root.errorText = root.connectionError;
        }
    }
    Process {
        id: wifiQuery
        command: ["nmcli", "--terse", "--escape", "yes", "--fields", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "auto"]
        stdout: StdioCollector {
            onStreamFinished: {
                const rows = []; const names = new Set();
                for (const line of text.trim().split("\n")) {
                    const fields = root.splitNetworkLine(line);
                    if (fields.length !== 4 || !fields[1] || names.has(fields[1])) continue;
                    names.add(fields[1]);
                    rows.push({name: fields[1], connected: fields[0] === "*", signal: Number(fields[2]), security: fields[3] || "Open"});
                }
                const sorted = rows.sort((a, b) => Number(b.connected) - Number(a.connected) || b.signal - a.signal);
                if (JSON.stringify(sorted) !== JSON.stringify(root.wirelessNetworks)) root.wirelessNetworks = sorted;
            }
        }
    }
    // Only soften an edge when there are more rows beyond it. Keep the page
    // header and segmented control outside this scrolling texture.
    Rectangle {
        id: deviceListMask
        width: deviceList.width
        height: deviceList.height
        visible: false
        layer.enabled: true
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 1 - deviceList.topFade) }
            GradientStop { position: Math.min(0.5, 16 / Math.max(1, deviceList.height)); color: "white" }
            GradientStop { position: Math.max(0.5, 1 - 20 / Math.max(1, deviceList.height)); color: "white" }
            GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 1 - deviceList.bottomFade) }
        }
    }
    PwObjectTracker { objects: root.active && root.page === "audio" ? root.audioOutputs.concat(root.audioInputs) : [] }
    ColumnLayout {
        id: detailLayout
        anchors.fill: parent
        // Detail pages use one flat reading plane. Rows own their interaction
        // state, so the page does not turn each section into another bubble.
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            IconButton {
                id: back
                objectName: "controlDetailBack"
                iconName: "go-previous-symbolic"
                label: "Back to Control Centre"
                onClicked: root.backRequested()
            }
            Text { Layout.fillWidth: true; text: root.page === "network" ? "Network" : root.page === "bluetooth" ? "Bluetooth" : root.page === "audio" ? "Sound" : "Display"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.DemiBold }
            ControlSwitch {
                objectName: "controlBluetoothPower"
                visible: root.page === "bluetooth"
                enabled: !!root.bluetoothAdapter
                checked: !!root.bluetoothAdapter && root.bluetoothAdapter.enabled
                Accessible.name: "Bluetooth"
                onToggled: if (root.bluetoothAdapter) root.bluetoothAdapter.enabled = checked
            }
        }
        SegmentedControl {
            visible: root.page === "audio"
            Layout.fillWidth: true
            options: ["output", "input"]
            currentValue: root.audioTab
            objectNamePrefix: "controlAudioTab_"
            onSelected: value => root.audioTab = value
        }
        AudioLevel { visible: root.page === "audio" && root.audioTab === "output"; node: root.outputNode; maximum: 1.5; label: "Output"; iconName: "audio-volume-high-symbolic" }
        AudioLevel { objectName: "controlMicrophone"; visible: root.page === "audio" && root.audioTab === "input"; node: root.inputNode; label: "Microphone"; iconName: "audio-input-microphone-symbolic" }
        Flickable {
            id: deviceList
            objectName: "controlDeviceList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: sections.implicitHeight + 8
            Layout.minimumHeight: 0
            clip: true
            contentWidth: width
            contentHeight: sections.implicitHeight + 8
            readonly property bool scrollable: contentHeight > height + 1
            readonly property real topFade: Math.min(1, Math.max(0, contentY - originY) / 16)
            readonly property real bottomFade: Math.min(1, Math.max(0, contentHeight - 8 - height - (contentY - originY)) / 20)
            flickableDirection: Flickable.VerticalFlick
            layer.enabled: scrollable && (topFade > 0 || bottomFade > 0)
            layer.effect: MultiEffect {
                maskEnabled: true
                maskSource: deviceListMask
                maskThresholdMin: 0.5
                maskSpreadAtMin: 1
            }
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {
                objectName: "controlDeviceScrollbar"
                parent: root
                x: deviceList.x + deviceList.width + 4
                y: deviceList.y
                height: deviceList.height
                policy: ScrollBar.AsNeeded
                width: 8
                padding: 2
                contentItem: Rectangle {
                    implicitWidth: 4
                    radius: 2
                    color: parent.pressed ? Theme.text : Theme.muted
                    opacity: parent.size < 1 ? 0.8 : 0
                }
            }
            ColumnLayout {
                id: sections
                objectName: "controlDeviceRows"
                width: deviceList.width
                spacing: 12
                DetailSection {
                    visible: root.page === "network" && root.networkSections.current.length > 0
                    title: "Connected"
                    Repeater {
                        model: root.networkSections.current
                        ConnectionEntry { required property var modelData; connection: modelData }
                    }
                }
                DetailSection {
                    visible: root.page === "network" && (root.networkSections.nearby.length > 0 || root.connections.some(connection => connection.type === "802-11-wireless"))
                    title: "Wi-Fi networks"
                    Repeater {
                        model: root.networkSections.nearby
                        ControlRow {
                            required property var modelData
                            title: modelData.name
                            subtitle: modelData.security === "Open" ? "Open network" : "Secured · " + modelData.security
                            navigation: true
                            iconName: "network-wireless-signal-" + (modelData.signal > 75 ? "excellent" : modelData.signal > 50 ? "good" : modelData.signal > 25 ? "ok" : "weak") + "-symbolic"
                            onClicked: root.settingsRequested("network")
                        }
                    }
                    EmptyMessage { visible: root.networkSections.nearby.length === 0; text: wifiQuery.running ? "Looking for networks…" : "No other networks nearby" }
                }
                DetailSection {
                    visible: root.page === "network" && root.networkSections.other.length > 0
                    title: "Saved networks"
                    Repeater { model: root.networkSections.other; ConnectionEntry { required property var modelData; connection: modelData } }
                }
                EmptyMessage { visible: root.page === "network" && networkQuery.running && root.connections.length === 0; text: "Loading connections…" }
                EmptyMessage { visible: root.page === "network" && !networkQuery.running && root.networkSections.current.length === 0; text: "No active network connection" }
                EmptyMessage {
                    visible: root.page === "bluetooth" && (!root.bluetoothAdapter || !root.bluetoothAdapter.enabled)
                    text: root.bluetoothAdapter ? "Bluetooth is off" : "Bluetooth is unavailable"
                }
                DetailSection {
                    visible: root.page === "bluetooth" && !!root.bluetoothAdapter && root.bluetoothAdapter.enabled
                    title: "My devices"
                    Repeater {
                        model: root.bluetoothDevices
                        ControlRow {
                            required property var modelData
                            title: modelData.name || modelData.deviceName
                            subtitle: modelData.state === BluetoothDeviceState.Connecting ? "Connecting…" : modelData.state === BluetoothDeviceState.Disconnecting ? "Disconnecting…" : modelData.connected ? "Connected" : ""
                            selected: modelData.connected
                            enabled: modelData.state !== BluetoothDeviceState.Connecting && modelData.state !== BluetoothDeviceState.Disconnecting
                            iconName: DetailModel.bluetoothIcon(modelData.icon)
                            Accessible.description: modelData.connected ? "Disconnect device" : "Connect device"
                            onClicked: if (modelData.connected) modelData.disconnect(); else modelData.connect()
                        }
                    }
                    EmptyMessage { visible: root.bluetoothDevices.length === 0; text: "No paired devices" }
                }
                DetailSection {
                    objectName: "controlBluetoothNearby"
                    visible: root.page === "bluetooth" && !!root.bluetoothAdapter && root.bluetoothAdapter.enabled
                    title: "Nearby devices"
                    ControlRow {
                        objectName: "controlBluetoothDiscover"
                        title: root.discoveryAdapter ? "Stop searching" : root.discoveryRunning ? "Searching…" : "Search for devices"
                        iconName: root.discoveryRunning ? "process-stop-symbolic" : "view-refresh-symbolic"
                        enabled: !!root.discoveryAdapter || !root.discoveryRunning
                        onClicked: root.toggleDiscovery()
                    }
                    Repeater {
                        model: root.nearbyDevices
                        ControlRow {
                            required property var modelData
                            title: modelData.name || modelData.deviceName
                            actionLabel: "Pair…"
                            iconName: DetailModel.bluetoothIcon(modelData.icon)
                            onClicked: root.settingsRequested("bluetooth")
                            onActionTriggered: root.settingsRequested("bluetooth")
                        }
                    }
                    EmptyMessage { visible: root.nearbyDevices.length === 0; text: root.discoveryRunning ? "Searching for devices…" : "No nearby devices found" }
                }
                DetailSection {
                    visible: root.page === "audio"
                    title: root.audioTab === "output" ? "Play sound through" : "Use microphone"
                    Repeater {
                        model: root.audioTab === "output" ? root.audioOutputs : root.audioInputs
                        ControlRow {
                            required property var modelData
                            title: modelData.description || modelData.name
                            selected: modelData === (root.audioTab === "output" ? root.outputNode : root.inputNode)
                            iconName: root.audioTab === "output" ? "audio-speakers-symbolic" : "audio-input-microphone-symbolic"
                            onClicked: if (root.audioTab === "output") root.outputSelected(modelData); else root.inputSelected(modelData)
                        }
                    }
                    EmptyMessage { visible: (root.audioTab === "output" ? root.audioOutputs : root.audioInputs).length === 0; text: root.audioTab === "output" ? "No output devices" : "No microphones" }
                }
                EmptyMessage { visible: root.page === "display"; text: "Adjust brightness, resolution and connected screens in Display settings." }
                EmptyMessage { visible: root.errorText.length > 0; text: root.errorText }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.outline; opacity: 0.4 }
        ControlRow {
            objectName: "controlDetailSettings"
            title: root.page === "network" ? "Network settings…" : root.page === "bluetooth" ? "Bluetooth settings…" : root.page === "audio" ? "Sound settings…" : "Display settings…"
            iconName: ""
            navigation: true
            onClicked: root.settingsRequested(root.page === "audio" ? "sound" : root.page)
        }
    }
    component EmptyMessage: Text {
        Layout.fillWidth: true
        Layout.margins: 10
        wrapMode: Text.Wrap
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    component DetailSection: ColumnLayout {
        id: section
        default property alias rows: rowLayout.data
        property string title: ""
        Layout.fillWidth: true
        spacing: 4
        data: [Text {
            visible: section.title.length > 0
            Layout.leftMargin: 4
            Layout.topMargin: 2
            text: section.title
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.Medium
        }, ColumnLayout {
            id: rowLayout
            Layout.fillWidth: true
            spacing: 2
        }]
    }
    component ConnectionEntry: ControlRow {
        required property var connection
        title: DetailModel.connectionName(connection)
        subtitle: root.pendingConnection === connection.uuid ? (root.networkAction === "up" ? "Connecting…" : "Disconnecting…")
            : connection.connected ? (connection.type === "802-3-ethernet" ? connection.name + " · Connected" : "Connected") : ""
        navigation: !connection.connected
        leadingBadge: connection.connected
        tileSurface: connection.connected
        actionLabel: connection.connected ? "Disconnect" : ""
        selected: connection.connected
        iconName: DetailModel.connectionIcon(connection)
        enabled: !connectNetwork.running
        focusPolicy: connection.connected ? Qt.NoFocus : Qt.StrongFocus
        Accessible.description: connection.connected ? "Current connection" : "Connect network"
        onClicked: if (!connection.connected) changeConnection()
        onActionTriggered: changeConnection()
        function changeConnection() {
            const result = root.connectionForCommand(connection.uuid, connection.connected ? "down" : "up");
            if (!result.ok) root.errorText = result.error;
        }
    }
}
