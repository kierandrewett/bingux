import QtQuick

// A labelled switch with an optional note below. Set enabled to false for a
// setting the protocol cannot carry yet; the label and switch then dim.
Column {
    id: root

    property string label: ""
    property string note: ""
    property bool checked: false
    signal toggled(bool value)

    width: parent ? parent.width : 300
    spacing: 2

    Item {
        width: root.width
        height: Math.max(labelText.implicitHeight, toggle.implicitHeight)

        Text {
            id: labelText
            anchors.left: parent.left
            anchors.right: toggle.left
            anchors.rightMargin: Theme.gap
            anchors.verticalCenter: parent.verticalCenter
            text: root.label
            color: root.enabled ? Theme.text : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(18 * InstallerState.uiScale)
            wrapMode: Text.WordWrap
        }
        ControlSwitch {
            id: toggle
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            checked: root.checked
            enabled: root.enabled
            onToggled: root.toggled(checked)
        }
    }
    Text {
        visible: root.note !== ""
        width: root.width
        text: root.note
        color: Theme.muted
        wrapMode: Text.WordWrap
        font.family: Theme.fontFamily
        font.pixelSize: Math.round(15 * InstallerState.uiScale)
    }
}
