import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import "DesktopLayout.js" as DesktopLayout

// The top bar of the lock screen and the greeter: the same widget ids and
// saved order as the desktop bar, filtered by DesktopLayout.allowed() for
// the current SessionContext. It is an Item inside SessionScreen, not a
// window, because lock surfaces cannot parent popups; its menus are
// SessionPopovers drawn in the same surface.
//
// Only read-only status and two actions survive here: choosing the keyboard
// layout (which Gnoblin applies to the greeter or the locked session itself)
// and the power actions SessionServices permits. Nothing launches, nothing
// shows notification content, nothing reaches the user's settings.
Item {
    id: root

    required property Item backdrop
    // Metrics from bingux-statusd, for keyboard layouts. Null hides them.
    property var metrics: null
    property string gnoblinCtlPath: "gnoblinctl"
    property bool interactive: true
    readonly property color ink: "#ffffff"
    readonly property string openMenu: keyboardMenu.open ? "keyboard" : controlsMenu.open ? "controls" : ""
    property alias keyboardOpen: keyboardMenu.open
    property alias controlsOpen: controlsMenu.open
    // The user's saved order for the right zone, or the defaults; the lock
    // runs as the user, the greeter sees the packaged defaults.
    readonly property var order: {
        const layout = DesktopEditing.desktop?.layout;
        const zone = layout && Array.isArray(layout["top-right"]) ? layout["top-right"] : DesktopLayout.defaults()["top-right"];
        return zone.filter(id => DesktopLayout.allowed(id, SessionContext.mode));
    }
    readonly property bool showKeyboard: order.includes("keyboard") && metrics !== null && metrics.desktopStateAvailable && metrics.inputSources.length > 0
    readonly property bool showControls: order.includes("controls")

    // Network through nmcli, like the desktop bar's indicators.
    property string networkState: "unknown"
    readonly property var battery: UPower.displayDevice
    readonly property bool batteryAvailable: battery !== null && battery.ready && battery.isLaptopBattery
    readonly property var audioSink: Pipewire.defaultAudioSink
    readonly property bool audioAvailable: audioSink !== null && audioSink.ready && audioSink.audio !== null
    readonly property bool volumeAllowed: audioAvailable && SessionServices.allows("volume")

    implicitHeight: Theme.barHeight + 8

    function closeMenus() {
        keyboardMenu.open = false;
        controlsMenu.open = false;
    }

    function networkIconName() {
        switch (networkState) {
        case "wifi":
            return "network-wireless-signal-excellent-symbolic";
        case "wired":
            return "network-wired-symbolic";
        case "vpn":
            return "network-vpn-symbolic";
        case "network":
            return "network-transmit-receive-symbolic";
        }
        return "network-offline-symbolic";
    }
    function networkLabel() {
        switch (networkState) {
        case "wifi":
            return "Wireless network connected";
        case "wired":
            return "Wired network connected";
        case "vpn":
            return "VPN connected";
        case "network":
            return "Network connected";
        case "offline":
            return "No network connection";
        }
        return "Network status unavailable";
    }
    function batteryIconName() {
        if (!batteryAvailable)
            return "";
        const percent = Math.round(battery.percentage * 100);
        const level = percent >= 95 ? "full" : percent >= 55 ? "good" : percent >= 25 ? "low" : percent >= 10 ? "caution" : "empty";
        return "battery-" + level + (UPower.onBattery ? "" : "-charging") + "-symbolic";
    }
    function batteryLabel() {
        if (!batteryAvailable)
            return "Battery status unavailable";
        const percent = Math.round(battery.percentage * 100);
        return "Battery " + percent + "%" + (UPower.onBattery ? "" : ", charging");
    }
    function updateNetworkState(output) {
        let next = "offline";
        for (const line of output.trim().split("\n")) {
            const fields = line.split(":");
            if (fields.length < 3 || !fields[2].startsWith("connected"))
                continue;
            if (fields[1] === "wifi" || fields[1] === "802-11-wireless") {
                next = "wifi";
                break;
            }
            if (fields[1] === "ethernet")
                next = "wired";
            else if (next === "offline")
                next = fields[1] === "tun" ? "vpn" : "network";
        }
        networkState = next;
    }
    function selectSource(source) {
        if (inputProcess.running)
            return;
        inputProcess.exec([root.gnoblinCtlPath, "input", "select", source.type, source.id]);
        keyboardMenu.open = false;
    }
    function powerAction(action) {
        if (!SessionServices.allows("power") || power.running)
            return;
        power.command = ["systemctl", action];
        power.running = true;
        controlsMenu.open = false;
    }

    Process {
        id: networkProcess
        command: ["timeout", "2s", "nmcli", "-t", "-f", "DEVICE,TYPE,STATE", "device"]
        stdout: StdioCollector {
            onStreamFinished: root.updateNetworkState(this.text)
        }
        onExited: code => {
            if (code !== 0)
                root.networkState = "unknown";
        }
        Component.onCompleted: running = true
    }
    Timer {
        interval: 5000
        repeat: true
        running: root.visible
        onTriggered: if (!networkProcess.running)
            networkProcess.running = true
    }
    Process {
        id: inputProcess
    }
    Process {
        id: power
        onExited: code => {
            if (code !== 0)
                powerError.text = "The power action was refused";
        }
    }

    // A soft shadow keeps the white glyphs readable over a bright sky.
    SessionShadow {
        id: cluster
        anchors.right: parent.right
        anchors.rightMargin: Theme.padding
        anchors.verticalCenter: parent.verticalCenter
        padding: 16
        blur: 0.5
        offsetY: 1
        colour: Qt.rgba(0, 0, 0, 0.45)
        visible: root.interactive
        RowLayout {
            spacing: 2

            SessionLink {
                objectName: "sessionBarKeyboard"
                visible: root.showKeyboard
                text: root.metrics ? root.metrics.inputSourceLabel.slice(0, 12) : ""
                dimmed: !keyboardMenu.open
                Accessible.name: "Keyboard layout " + text
                onClicked: {
                    controlsMenu.open = false;
                    keyboardMenu.open = !keyboardMenu.open;
                }
            }
            SessionLink {
                id: controlsButton
                objectName: "sessionBarControls"
                visible: root.showControls
                dimmed: !controlsMenu.open
                Accessible.name: "Status and power"
                Accessible.description: root.networkLabel() + "; " + root.batteryLabel()
                horizontalPadding: 8
                implicitWidth: statusRow.implicitWidth + 16
                onClicked: {
                    keyboardMenu.open = false;
                    controlsMenu.open = !controlsMenu.open;
                }
                contentItem: Row {
                    id: statusRow
                    anchors.centerIn: parent
                    spacing: 8
                    SymbolicIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: Theme.iconSize
                        source: AdwaitaIcons.source(root.networkIconName())
                        color: root.ink
                        opacity: root.networkState === "offline" || root.networkState === "unknown" ? 0.6 : 1
                    }
                    Text {
                        visible: root.batteryAvailable
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.batteryAvailable ? Math.round(root.battery.percentage * 100) + "%" : ""
                        color: root.ink
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                    SymbolicIcon {
                        visible: root.batteryAvailable
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: Theme.iconSize
                        source: root.batteryAvailable ? AdwaitaIcons.source(root.batteryIconName()) : ""
                        color: root.ink
                    }
                    SymbolicIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        implicitSize: Theme.iconSize
                        source: AdwaitaIcons.source("system-shutdown-symbolic")
                        color: root.ink
                        opacity: 0.85
                    }
                }
            }
        }
    }

    SessionPopover {
        id: keyboardMenu
        objectName: "sessionBarKeyboardMenu"
        anchors.right: cluster.right
        anchors.top: cluster.bottom
        anchors.topMargin: Theme.gap
        backdrop: root.backdrop
        width: 240
        transformOrigin: Item.Top
        ColumnLayout {
            width: parent.width
            spacing: 2
            Repeater {
                model: root.metrics ? root.metrics.inputSources : []
                SessionRow {
                    required property var modelData
                    Layout.fillWidth: true
                    text: modelData.displayName
                    selected: root.metrics.currentInputSource !== null && root.metrics.currentInputSource.type === modelData.type && root.metrics.currentInputSource.id === modelData.id
                    ticked: selected
                    enabled: !inputProcess.running
                    onClicked: root.selectSource(modelData)
                }
            }
        }
    }

    // Status rows, a volume control when the session allows it, and the power
    // actions. This is the lock screen's whole "control centre".
    SessionPopover {
        id: controlsMenu
        objectName: "sessionBarControlsMenu"
        anchors.right: cluster.right
        anchors.top: cluster.bottom
        anchors.topMargin: Theme.gap
        backdrop: root.backdrop
        width: 260
        transformOrigin: Item.Top
        ColumnLayout {
            width: parent.width
            spacing: 2
            SessionRow {
                Layout.fillWidth: true
                iconName: root.networkIconName()
                text: root.networkLabel()
                enabled: false
            }
            SessionRow {
                Layout.fillWidth: true
                visible: root.batteryAvailable
                iconName: root.batteryIconName()
                text: root.batteryLabel()
                enabled: false
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: 6
                Layout.rightMargin: 10
                Layout.topMargin: 4
                Layout.bottomMargin: 4
                visible: root.volumeAllowed
                spacing: 10
                SymbolicIcon {
                    implicitSize: Theme.iconSize
                    source: AdwaitaIcons.source(root.audioAvailable && root.audioSink.audio.muted ? "audio-volume-muted-symbolic" : "audio-volume-high-symbolic")
                    color: root.ink
                }
                SessionSlider {
                    objectName: "sessionBarVolume"
                    Layout.fillWidth: true
                    value: root.audioAvailable ? root.audioSink.audio.volume : 0
                    onMoved: value => {
                        if (!root.volumeAllowed)
                            return;
                        root.audioSink.audio.muted = false;
                        root.audioSink.audio.volume = value;
                    }
                }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.bottomMargin: 4
                height: 1
                color: Qt.rgba(1, 1, 1, 0.14)
            }
            SessionRow {
                objectName: "sessionBarSuspend"
                Layout.fillWidth: true
                visible: SessionContext.locked
                iconName: "weather-clear-night-symbolic"
                text: "Sleep"
                enabled: !power.running
                onClicked: root.powerAction("suspend")
            }
            SessionRow {
                objectName: "sessionBarRestart"
                Layout.fillWidth: true
                iconName: "view-refresh-symbolic"
                text: "Restart…"
                enabled: !power.running
                onClicked: root.powerAction("reboot")
            }
            SessionRow {
                objectName: "sessionBarShutDown"
                Layout.fillWidth: true
                iconName: "system-shutdown-symbolic"
                text: "Shut Down…"
                enabled: !power.running
                onClicked: root.powerAction("poweroff")
            }
            Text {
                id: powerError
                Layout.fillWidth: true
                Layout.leftMargin: 10
                visible: text !== ""
                color: Theme.danger
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
                wrapMode: Text.Wrap
            }
        }
    }
    PwObjectTracker {
        objects: root.audioSink ? [root.audioSink] : []
    }
}
