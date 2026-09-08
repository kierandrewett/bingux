import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Widgets

AbstractButton {
    id: root
    required property string iconName
    required property string label
    property url imageSource: ""
    property bool highlighted: false
    implicitWidth: 32
    implicitHeight: 32
    hoverEnabled: true
    Accessible.name: label
    background: ControlCentreButtonSurface { control: root; selected: root.highlighted }
    contentItem: Item {
        scale: root.down ? 0.88 : 1
        Behavior on scale { NumberAnimation { duration: Theme.reducedMotion ? 0 : 80; easing.type: Easing.OutCubic } }
        SymbolicIcon { visible: avatar.status !== Image.Ready; anchors.centerIn: parent; implicitSize: 16; source: Quickshell.iconPath(root.iconName); color: root.enabled ? Theme.text : Theme.muted }
        ClippingRectangle {
            anchors.centerIn: parent
            width: 24; height: 24; radius: 12
            visible: avatar.status === Image.Ready
            color: "transparent"
            Image { id: avatar; anchors.fill: parent; source: root.imageSource; fillMode: Image.PreserveAspectCrop; sourceSize.width: 48; sourceSize.height: 48 }
        }
    }
    ShellTooltip { parent: root; visible: root.visible && root.enabled && (root.hovered || root.visualFocus); text: root.label }
}
