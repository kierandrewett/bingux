import QtQuick
import QtQuick.Controls
import Quickshell

AbstractButton {
    id: root
    required property string iconName
    property bool selected: false
    property string description: ""
    implicitHeight: description ? 60 : 44
    padding: 0
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.PageTab
    Accessible.name: text
    Accessible.selected: selected
    background: ControlCentreButtonSurface {
        control: root
        radius: 8
        baseColor: root.selected ? Theme.settingsSelection : "transparent"
    }
    contentItem: Item {
        SymbolicIcon {
            id: icon
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            implicitSize: 16
            source: Quickshell.iconPath(root.iconName)
            color: Theme.text
        }
        Text {
            id: titleLabel
            anchors.left: icon.right
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: root.description ? -9 : 0
            text: root.text
            textFormat: Text.PlainText
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            color: Theme.text
            elide: Text.ElideRight
        }
        Text {
            anchors.left: titleLabel.left
            anchors.right: titleLabel.right
            anchors.top: titleLabel.bottom
            anchors.topMargin: 2
            visible: root.description !== ""
            text: root.description
            textFormat: Text.PlainText
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            color: Theme.muted
            elide: Text.ElideRight
        }
    }
}
