import QtQuick
import QtQuick.Controls
import ".."

// A frosted pill button for the authentication screens: retry, go back,
// confirm. `primary` fills it a little more. Keyboard focus draws an outline.
AbstractButton {
    id: root

    property AuthTheme theme: AuthTheme {}
    property Item backdrop: null
    property bool primary: false
    property string iconName: ""

    horizontalPadding: 16
    implicitHeight: 32
    implicitWidth: row.implicitWidth + horizontalPadding * 2
    hoverEnabled: true
    activeFocusOnTab: true
    opacity: enabled ? 1 : 0.5
    Accessible.role: Accessible.Button
    Accessible.name: text

    background: SessionGlass {
        backdrop: root.backdrop
        tint: root.down ? Qt.rgba(1, 1, 1, 0.34) : root.hovered || root.visualFocus ? Qt.rgba(1, 1, 1, 0.26) : root.primary ? Qt.rgba(1, 1, 1, 0.22) : root.theme.glass
        outline: root.visualFocus ? root.theme.glassOutlineFocus : root.theme.glassOutline
    }
    contentItem: Item {
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6
            SymbolicIcon {
                visible: root.iconName !== ""
                anchors.verticalCenter: parent.verticalCenter
                implicitSize: Theme.iconSize
                source: root.iconName !== "" ? AdwaitaIcons.source(root.iconName) : ""
                color: root.theme.ink
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: root.theme.ink
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall + 1
                font.weight: root.primary ? Font.DemiBold : Font.Normal
            }
        }
    }
    Keys.onReturnPressed: root.clicked()
    Keys.onEnterPressed: root.clicked()
}
