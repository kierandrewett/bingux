import QtQuick
import QtQuick.Controls

// A text entry in the greeter's style, with an optional caption above it and
// a message below it. The message is red when `invalid` is true.
Column {
    id: root

    property string caption: ""
    property alias text: field.text
    property alias placeholderText: field.placeholderText
    property alias echoMode: field.echoMode
    property alias inputMethodHints: field.inputMethodHints
    property alias validator: field.validator
    property alias maximumLength: field.maximumLength
    property alias acceptableInput: field.acceptableInput
    property alias textField: field
    property string message: ""
    property bool invalid: false
    property bool readOnly: false
    property int fieldHeight: 36

    signal accepted
    signal edited
    width: parent ? parent.width : 300
    spacing: 4

    Text {
        visible: root.caption !== ""
        text: root.caption
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }

    TextField {
        id: field
        width: parent.width
        height: root.fieldHeight
        leftPadding: 14
        rightPadding: 14
        readOnly: root.readOnly
        color: Theme.text
        placeholderTextColor: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall + 1
        selectByMouse: true
        background: Rectangle {
            radius: height / 2
            color: Theme.elevated
            border.width: 1
            border.color: root.invalid ? Theme.danger : field.activeFocus ? Theme.accent : Qt.rgba(1, 1, 1, 0.12)
        }
        Accessible.name: placeholderText !== "" ? placeholderText : root.caption
        onAccepted: root.accepted()
        onTextEdited: root.edited()
    }

    Text {
        visible: root.message !== ""
        width: parent.width
        text: root.message
        wrapMode: Text.Wrap
        color: root.invalid ? Theme.danger : Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
}
