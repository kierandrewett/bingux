import QtQuick
import QtQuick.Controls
import "auth"

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
    // Opt-in sizing for large rows (installer options): the row height, a
    // second line under the label, the label size and the left inset.
    property int rowHeight: 36
    property string detail: ""
    property int labelPixelSize: Theme.fontSmall + 1
    property int contentInset: 6

    implicitHeight: rowHeight
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
            anchors.leftMargin: root.contentInset
            anchors.verticalCenter: parent.verticalCenter
            spacing: 10
            UserAvatar {
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
            Column {
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    text: root.text
                    color: "#ffffff"
                    font.family: Theme.fontFamily
                    font.pixelSize: root.labelPixelSize
                    font.weight: root.selected ? Font.DemiBold : Font.Normal
                }
                Text {
                    visible: root.detail !== ""
                    text: root.detail
                    color: Qt.rgba(1, 1, 1, 0.62)
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(root.labelPixelSize * 0.82)
                }
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
