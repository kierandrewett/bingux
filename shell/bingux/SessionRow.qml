import QtQuick
import QtQuick.Controls
import "auth"

// One row in a SessionPopover list: an avatar or icon, a label and an
// optional tick. States, weakest to strongest fill: rest, hover, selected,
// selected and hovered, pressed. The selected row also carries an accent bar
// at its leading edge, so it reads without the tick. Keyboard focus draws a
// ring; it never borrows the hover fill.
AbstractButton {
    id: root

    property url avatarSource: ""
    property string avatarName: ""
    property string iconName: ""
    property bool selected: false
    // An action that ends the session (Restart, Shut Down): danger ink and a
    // faint danger tint at rest.
    property bool destructive: false
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
        color: root.down ? Qt.rgba(1, 1, 1, 0.30) : root.selected ? Qt.rgba(1, 1, 1, root.hovered ? 0.26 : 0.20) : root.hovered ? Qt.rgba(1, 1, 1, 0.12) : root.destructive ? Qt.rgba(Theme.danger.r, Theme.danger.g, Theme.danger.b, 0.14) : "transparent"
        border.width: root.visualFocus ? 2 : 0
        border.color: "#ffffff"
        Behavior on color {
            ColorAnimation {
                duration: Theme.motion
            }
        }
        Rectangle {
            objectName: "sessionRowMarker"
            visible: root.selected
            x: 3
            anchors.verticalCenter: parent.verticalCenter
            width: 3
            height: parent.height * 0.5
            radius: 1.5
            color: Theme.accent
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
                    source: root.iconName !== "" ? BinguxIcons.source(root.iconName) : ""
                    color: root.destructive ? Theme.danger : "#ffffff"
                }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    textFormat: Text.PlainText
                    text: root.text
                    color: root.destructive ? Theme.danger : "#ffffff"
                    font.family: Theme.fontFamily
                    font.pixelSize: root.labelPixelSize
                    font.weight: root.selected ? Font.DemiBold : Font.Normal
                }
                Text {
                    textFormat: Text.PlainText
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
            source: BinguxIcons.source("object-select-symbolic")
            color: "#ffffff"
        }
    }
}
