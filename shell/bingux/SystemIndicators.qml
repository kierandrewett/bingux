import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Widgets
import Quickshell.Bluetooth

Item {
    id: root
    readonly property int controlSize: 24
    property string timeoutPath: "timeout"
    property var services: ControlCentreServices
    Binding { target: root.services; property: "statusVisible"; value: root.visible }
    readonly property var extraIndicators: {
        const rows = [];
        const service = root.services;
        const state = service.state;
        if (root.microphoneMuted) rows.push({key: "microphone", icon: "microphone-sensitivity-muted-symbolic",
            active: true, label: "Microphone muted"});
        if (service.showControl("vpn") && service.vpns.length) {
            const connected = service.vpns.filter(vpn => vpn.connected);
            rows.push({key: "vpn", icon: "network-vpn-symbolic", active: connected.length > 0,
                label: "VPN: " + (connected.length ? connected.map(vpn => vpn.name + ", " + vpn.subtitle).join("; ") : "Disconnected")});
        }
        if (service.showControl("dnd")) rows.push({key: "dnd", icon: "notifications-disabled-symbolic",
            active: service.doNotDisturb, label: "Do Not Disturb: " + (service.doNotDisturb ? "On" : "Off")});
        if (service.showControl("nightLight")) rows.push({key: "nightLight", icon: "night-light-symbolic",
            active: !!state.nightLightActive, label: "Night Light: " + (!state.nightLightAvailable ? "Unavailable" : state.nightLightActive ? "On" : state.nightLight ? "Scheduled" : "Off")});
        if (service.showControl("power")) {
            const power = state.power || {};
            const profile = ["power-saver", "balanced", "performance"].includes(power.profile) ? power.profile : "balanced";
            rows.push({key: "power", icon: "power-profile-" + profile + "-symbolic", active: !!power.available,
                label: "Power mode: " + (!power.available ? "Unavailable" : profile === "power-saver" ? "Power Saver" : profile === "performance" ? "Performance" : "Balanced")});
        }
        if (service.showControl("awake") || service.keepAwake) rows.push({key: "awake", icon: "display-brightness-symbolic",
            active: service.keepAwake, label: "Keep Awake: " + (service.keepAwake ? "On" : "Off")});
        return rows;
    }
    readonly property string extraStatusDescription: extraIndicators.filter(indicator => indicator.active).map(indicator => indicator.label).join("\n")
    readonly property var audioSink: Pipewire.defaultAudioSink
    readonly property var audioSource: Pipewire.defaultAudioSource
    readonly property bool microphoneMuted: !!audioSource && audioSource.ready && !!audioSource.audio && audioSource.audio.muted
    readonly property var battery: UPower.displayDevice
    readonly property var bluetoothAdapter: Bluetooth.defaultAdapter
    readonly property int connectedBluetoothDevices: bluetoothAdapter ? bluetoothAdapter.devices.values.filter(device => device.connected).length : 0
    property string networkState: "unknown"
    readonly property bool audioAvailable: audioSink !== null && audioSink.ready && audioSink.audio !== null
    readonly property bool audioMuted: audioAvailable && audioSink.audio.muted
    readonly property real audioVolume: audioAvailable ? audioSink.audio.volume : 0
    readonly property bool laptopBatteryAvailable: battery !== null && battery.ready && battery.isLaptopBattery

    function updateNetworkState(output) {
        let nextState = "offline";
        const lines = output.trim().split("\n");

        for (let index = 0; index < lines.length; index += 1) {
            const fields = lines[index].split(":");

            if (fields.length < 3 || !fields[2].startsWith("connected")) {
                continue;
            }

            if (fields[1] === "wifi" || fields[1] === "802-11-wireless") {
                nextState = "wifi";
                break;
            }

            if (fields[1] === "ethernet") {
                nextState = "wired";
                continue;
            }

            if (fields[1] === "tun" && nextState === "offline") {
                nextState = "vpn";
            } else if (nextState === "offline") {
                nextState = "network";
            }
        }

        networkState = nextState;
    }

    function networkIconName() {
        if (networkState === "wifi") {
            return "network-wireless-signal-excellent-symbolic";
        }

        if (networkState === "wired") {
            return "network-wired-symbolic";
        }

        if (networkState === "vpn") {
            return "network-vpn-symbolic";
        }

        if (networkState === "network") {
            return "network-transmit-receive-symbolic";
        }

        return "network-offline-symbolic";
    }

    function networkAccessibleName() {
        if (networkState === "wifi") {
            return "Wireless network connected";
        }

        if (networkState === "wired") {
            return "Wired network connected";
        }

        if (networkState === "vpn") {
            return "VPN connected";
        }

        if (networkState === "network") {
            return "Network connected";
        }

        if (networkState === "offline") {
            return "No network connection";
        }

        return "Network status unavailable";
    }

    Process {
        id: networkProcess

        command: [root.timeoutPath, "2s", "nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device"]
        stdout: StdioCollector {
            onStreamFinished: root.updateNetworkState(this.text)
        }

        onExited: function(exitCode) {
            if (exitCode !== 0) {
                root.networkState = "unknown";
            }
        }

        Component.onCompleted: running = true
    }

    Timer {
        interval: 5000
        repeat: true
        running: true
        onTriggered: {
            if (!networkProcess.running) {
                networkProcess.running = true;
            }
        }
    }

    implicitWidth: Math.max(0, indicatorRow.implicitWidth - Theme.spaceSmall)
    implicitHeight: controlSize
    width: implicitWidth
    height: implicitHeight


    function audioIconName() {
        if (!audioAvailable || audioMuted || audioVolume <= 0.01) {
            return "audio-volume-muted-symbolic";
        }

        if (audioVolume < 0.34) {
            return "audio-volume-low-symbolic";
        }

        if (audioVolume < 0.67) {
            return "audio-volume-medium-symbolic";
        }

        return "audio-volume-high-symbolic";
    }

    function audioAccessibleName() {
        if (!audioAvailable) {
            return "No active audio output";
        }

        if (audioMuted) {
            return "Audio muted";
        }

        return "Audio volume " + Math.round(audioVolume * 100) + " percent";
    }

    function batteryAccessibleName() {
        if (!laptopBatteryAvailable) {
            return "Battery status unavailable";
        }

        const percentage = Math.round(battery.percentage);
        return UPower.onBattery ? "Battery " + percentage + " percent, discharging" : "Battery " + percentage + " percent, charging";
    }

    function changeVolume(delta) {
        if (!audioAvailable) {
            return;
        }

        audioSink.audio.muted = false;
        audioSink.audio.volume = Math.max(0, Math.min(1, audioSink.audio.volume + delta));
    }

    PwObjectTracker {
        objects: [root.audioSink, root.audioSource].filter(node => node !== null)
    }

    Row {
        id: indicatorRow
        spacing: 0
        StatusIndicator {
            objectName: "controlStatus_network"
            slotSize: root.controlSize
            shown: root.networkState !== "offline" && root.networkState !== "unknown"
            iconName: root.networkIconName()
            label: root.networkAccessibleName()
        }
        StatusIndicator {
            objectName: "controlStatus_audio"
            slotSize: root.controlSize
            shown: root.audioAvailable && !root.audioMuted && root.audioVolume > 0.01
            iconName: root.audioIconName()
            label: root.audioAccessibleName()
            MouseArea {
                anchors.fill: parent
                enabled: parent.shown
                onClicked: if (root.audioAvailable) root.audioSink.audio.muted = !root.audioSink.audio.muted
                onWheel: wheel => { root.changeVolume(wheel.angleDelta.y > 0 ? 0.05 : -0.05); wheel.accepted = true; }
            }
        }
        StatusIndicator {
            objectName: "controlStatus_battery"
            slotSize: root.controlSize
            shown: root.laptopBatteryAvailable
            iconName: root.laptopBatteryAvailable ? root.battery.iconName : "battery-missing-symbolic"
            label: root.batteryAccessibleName()
        }
        StatusIndicator {
            objectName: "controlStatus_bluetooth"
            slotSize: root.controlSize
            shown: root.bluetoothAdapter !== null && root.bluetoothAdapter.enabled
            iconName: "bluetooth-active-symbolic"
            label: root.connectedBluetoothDevices > 0 ? "Bluetooth, " + root.connectedBluetoothDevices + " connected" : "Bluetooth on"
        }
        Repeater {
            // Stable keys retain exiting icons and do not replay other icons' animations.
            model: ["microphone", "vpn", "dnd", "nightLight", "power", "awake"]
            StatusIndicator {
                required property string modelData
                readonly property var status: root.extraIndicators.find(indicator => indicator.key === modelData) || null
                objectName: "controlStatus_" + modelData
                slotSize: root.controlSize
                shown: status !== null && status.active
                iconName: status ? status.icon : ""
                label: status ? status.label : ""
            }
        }
    }
}
