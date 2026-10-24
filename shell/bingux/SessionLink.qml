import QtQuick
import QtQuick.Controls

// Small text or icon button for the lock screen and greeter. It has no
// resting surface so it sits quietly on the wallpaper; hover and press show a
// translucent white pill.
AbstractButton {
    id: root
    property string iconName: ""
    property string trailingIconName: ""
    property bool dimmed: false
    horizontalPadding: text !== "" ? 10 : 6
    implicitHeight: 28
    implicitWidth: contentRow.implicitWidth + horizontalPadding * 2
    hoverEnabled: true
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    background: Rectangle {
        radius: height / 2
        color: root.down ? Qt.rgba(1, 1, 1, 0.3) : root.hovered || root.visualFocus ? Qt.rgba(1, 1, 1, 0.18) : "transparent"
        border.width: root.visualFocus ? 1 : 0
        border.color: Qt.rgba(1, 1, 1, 0.5)
    }
    contentItem: Item {
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: 6
            SymbolicIcon {
                visible: root.iconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                implicitSize: Theme.iconSize
                source: root.iconName !== "" ? AdwaitaIcons.source(root.iconName) : ""
                color: "#ffffff"
                opacity: root.dimmed ? 0.7 : 1
            }
            Text {
                visible: root.text !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: "#ffffff"
                opacity: root.dimmed ? 0.7 : 1
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            SymbolicIcon {
                visible: root.trailingIconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                implicitSize: 12
                source: root.trailingIconName !== "" ? AdwaitaIcons.source(root.trailingIconName) : ""
                color: "#ffffff"
                opacity: 0.8
            }
        }
    }
}
