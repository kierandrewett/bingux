import QtQuick
import Quickshell
import Quickshell.Networking
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Widgets

Item {
    id: root

    readonly property int controlSize: 24
    readonly property var audioSink: Pipewire.defaultAudioSink
    readonly property var battery: UPower.displayDevice
    readonly property var connectedWifi: connectedDevice(DeviceType.Wifi)
    readonly property var connectedWired: connectedDevice(DeviceType.Wired)
    readonly property bool audioAvailable: audioSink !== null && audioSink.ready && audioSink.audio !== null
    readonly property bool audioMuted: audioAvailable && audioSink.audio.muted
    readonly property real audioVolume: audioAvailable ? audioSink.audio.volume : 0
    readonly property bool laptopBatteryAvailable: battery !== null && battery.ready && battery.isLaptopBattery

    implicitWidth: indicatorRow.implicitWidth
    implicitHeight: indicatorRow.implicitHeight
    width: implicitWidth
    height: implicitHeight

    function connectedDevice(type) {
        const devices = Networking.devices.values;

        for (let index = 0; index < devices.length; index += 1) {
            const device = devices[index];

            if (device.type === type && device.connected) {
                return device;
            }
        }

        return null;
    }

    function networkIconName() {
        if (connectedWifi !== null) {
            return "network-wireless-signal-excellent-symbolic";
        }

        if (connectedWired !== null) {
            return "network-wired-symbolic";
        }

        if (!Networking.wifiEnabled) {
            return "network-wireless-disabled-symbolic";
        }

        return "network-offline-symbolic";
    }

    function networkAccessibleName() {
        if (connectedWifi !== null) {
            return "Wireless network connected";
        }

        if (connectedWired !== null) {
            return "Wired network connected";
        }

        if (!Networking.wifiEnabled) {
            return "Wireless network disabled";
        }

        return "No network connection";
    }

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
