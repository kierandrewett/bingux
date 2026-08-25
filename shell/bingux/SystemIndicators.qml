import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Widgets

Item {
    id: root

    readonly property int controlSize: 24
    readonly property var audioSink: Pipewire.defaultAudioSink
    readonly property var battery: UPower.displayDevice
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

        command: [ "nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device" ]
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

    implicitWidth: indicatorRow.implicitWidth
    implicitHeight: indicatorRow.implicitHeight
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
        objects: root.audioSink === null ? [] : [root.audioSink]
    }

    Row {
        id: indicatorRow

        spacing: 4

        Item {
            width: root.controlSize
            height: root.controlSize
            Accessible.name: root.networkAccessibleName()
            Accessible.role: Accessible.StaticText

            IconImage {
                anchors.centerIn: parent
                implicitSize: 16
                source: Quickshell.iconPath(root.networkIconName(), "network-offline-symbolic")
            }
        }

        Item {
            width: root.controlSize
            height: root.controlSize
            visible: root.audioSink !== null
            Accessible.name: root.audioAccessibleName()
            Accessible.role: Accessible.Button

            IconImage {
                anchors.centerIn: parent
                implicitSize: 16
                source: Quickshell.iconPath(root.audioIconName(), "audio-volume-muted-symbolic")
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton

                onClicked: {
                    if (root.audioAvailable) {
                        root.audioSink.audio.muted = !root.audioSink.audio.muted;
                    }
                }

                onWheel: function(wheel) {
                    root.changeVolume(wheel.angleDelta.y > 0 ? 0.05 : -0.05);
                    wheel.accepted = true;
                }
            }
        }

        Item {
            width: root.controlSize
            height: root.controlSize
            visible: root.laptopBatteryAvailable
            Accessible.name: root.batteryAccessibleName()
            Accessible.role: Accessible.StaticText

            IconImage {
                anchors.centerIn: parent
                implicitSize: 16
                source: Quickshell.iconPath(root.battery.iconName, "battery-missing-symbolic")
            }
        }
    }
}
