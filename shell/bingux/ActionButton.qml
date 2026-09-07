import QtQuick
import QtQuick.Controls

AbstractButton {
    id: root
    implicitWidth: Math.max(36, label.implicitWidth + Theme.padding * 2)
    implicitHeight: 36
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    background: Rectangle {
        radius: Theme.radius
        color: root.down ? Theme.pressed : root.hovered ? Theme.hover : Theme.elevated
        border.width: root.activeFocus ? 2 : 0
        border.color: Theme.accent
    }
    contentItem: Text {
        id: label
        text: root.text
        textFormat: Text.PlainText
        color: root.enabled ? Theme.text : Theme.muted
        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
