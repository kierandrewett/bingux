import QtQuick
import QtQuick.Controls

// One row in a SessionPopover list: an avatar or icon, a label and an
// optional tick. The selected row carries a stronger translucent fill.
AbstractButton {
    id: root

    property url avatarSource: ""
    property string avatarName: ""
    property string iconName: ""
    property bool selected: false
    // Shows a tick on the right; used by single-choice lists. (`checked` is
    // AbstractButton state and would change the button semantics.)
    property bool ticked: false

    implicitHeight: 36
    hoverEnabled: true
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text

    background: Rectangle {
        radius: 9
        color: root.down ? Qt.rgba(1, 1, 1, 0.26) : root.hovered || root.visualFocus ? Qt.rgba(1, 1, 1, 0.16) : root.selected ? Qt.rgba(1, 1, 1, 0.1) : "transparent"
        Behavior on color {
            ColorAnimation {
                duration: Theme.motion
            }
        }
    }
    contentItem: Item {
        Row {
            anchors.left: parent.left
            anchors.leftMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            spacing: 10
            SessionAvatar {
                visible: root.iconName === "" && root.avatarName !== ""
                anchors.verticalCenter: parent.verticalCenter
                size: 24
                source: root.avatarSource
                name: root.avatarName
            }
            Rectangle {
                visible: root.iconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 24
                radius: 12
                color: Qt.rgba(1, 1, 1, 0.16)
                SymbolicIcon {
                    anchors.centerIn: parent
                    implicitSize: 12
                    source: root.iconName !== "" ? AdwaitaIcons.source(root.iconName) : ""
                    color: "#ffffff"
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: "#ffffff"
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall + 1
                font.weight: root.selected ? Font.DemiBold : Font.Normal
            }
        }
        SymbolicIcon {
            visible: root.ticked
            anchors.right: parent.right
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            implicitSize: 12
            source: AdwaitaIcons.source("object-select-symbolic")
            color: "#ffffff"
        }
    }
}
