import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Bluetooth

ShellPopup {
    id: root
    required property var indicators
    popupWidth: 380
    popupHeight: controls.implicitHeight + contentPadding * 2
    preferredX: width - popupWidth - Theme.padding
    function settings(panel) {
        Quickshell.execDetached(["gnome-control-center", panel]);
        visible = false;
    }
    ColumnLayout {
        id: controls
        width: parent.width
        spacing: Theme.padding
        RowLayout {
            Layout.fillWidth: true
            Text { Layout.fillWidth: true; text: "Control Centre"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.DemiBold }
            ActionButton { cornerRadius: root.contentRadius; text: "×"; Accessible.name: "Close control centre"; onClicked: root.visible = false }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            rowSpacing: Theme.gap
            columnSpacing: Theme.gap
            Tile {
                iconName: root.indicators.networkIconName()
                title: "Network"
                subtitle: root.indicators.networkAccessibleName()
                onClicked: root.settings("network")
            }
            Tile {
                iconName: "bluetooth-active-symbolic"
                title: "Bluetooth"
                enabled: Bluetooth.defaultAdapter !== null
                subtitle: Bluetooth.defaultAdapter ? (Bluetooth.defaultAdapter.enabled ? "On" : "Off") : "Unavailable"
                onClicked: if (Bluetooth.defaultAdapter) Bluetooth.defaultAdapter.enabled = !Bluetooth.defaultAdapter.enabled
            }
            Tile { iconName: "display-brightness-symbolic"; title: "Display"; subtitle: "Brightness and screens"; onClicked: root.settings("display") }
            Tile { iconName: "preferences-system-symbolic"; title: "Settings"; subtitle: "System preferences"; onClicked: root.settings("") }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.outline }
        RowLayout {
            Layout.fillWidth: true
            ActionButton { cornerRadius: root.contentRadius;
                Layout.preferredWidth: 40
                enabled: root.indicators.audioAvailable
                Accessible.name: root.indicators.audioMuted ? "Unmute" : "Mute"
                onClicked: root.indicators.audioSink.audio.muted = !root.indicators.audioMuted
                contentItem: Item { SymbolicIcon { anchors.centerIn: parent; source: Quickshell.iconPath(root.indicators.audioIconName()); implicitSize: 20 } }
            }
            Slider {
                id: volume
                Layout.fillWidth: true
                from: 0
                to: 1.5
                enabled: root.indicators.audioAvailable
                value: root.indicators.audioVolume
                Accessible.name: "Volume"
                onMoved: { root.indicators.audioSink.audio.muted = false; root.indicators.audioSink.audio.volume = value }
                background: Rectangle {
                    x: volume.leftPadding
                    y: volume.topPadding + (volume.availableHeight - height) / 2
                    width: volume.availableWidth
                    height: 6
                    radius: 3
                    color: Theme.elevated
                    Rectangle { width: volume.visualPosition * parent.width; height: parent.height; radius: parent.radius; color: Theme.accent }
                }
                handle: Rectangle {
                    x: volume.leftPadding + volume.visualPosition * (volume.availableWidth - width)
                    y: volume.topPadding + (volume.availableHeight - height) / 2
                    width: 18; height: 18; radius: 9
                    color: volume.pressed ? Theme.pressed : Theme.text
                    border.width: volume.activeFocus ? 2 : 0
                    border.color: Theme.accent
                }
            }
            Text { Layout.preferredWidth: 42; horizontalAlignment: Text.AlignRight; text: root.indicators.audioAvailable ? Math.round(root.indicators.audioVolume * 100) + "%" : "—"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
        }
        RowLayout {
            Layout.fillWidth: true
            Text { Layout.fillWidth: true; text: root.indicators.laptopBatteryAvailable ? root.indicators.batteryAccessibleName() : ""; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            ActionButton { cornerRadius: root.contentRadius; text: "Sound settings"; onClicked: root.settings("sound") }
            ActionButton { cornerRadius: root.contentRadius; text: "Lock"; onClicked: { Quickshell.execDetached(["loginctl", "lock-session"]); root.visible = false } }
        }
    }
    component Tile: AbstractButton {
        id: tile
        required property string iconName
        required property string title
        required property string subtitle
        Layout.fillWidth: true
        implicitHeight: 76
        activeFocusOnTab: true
        Accessible.name: title + ", " + subtitle
        background: Rectangle { radius: root.contentRadius; color: !tile.enabled ? Theme.surface : tile.down ? Theme.pressed : tile.hovered || tile.activeFocus ? Theme.hover : Theme.elevated }
        contentItem: RowLayout {
            spacing: Theme.padding
            anchors.fill: parent
            anchors.margins: Theme.padding
            SymbolicIcon { implicitSize: 20; source: Quickshell.iconPath(tile.iconName) }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSmall
                Text { Layout.fillWidth: true; text: tile.title; color: tile.enabled ? Theme.text : Theme.muted; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
                Text { Layout.fillWidth: true; text: tile.subtitle; color: Theme.muted; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            }
        }
    }
}
