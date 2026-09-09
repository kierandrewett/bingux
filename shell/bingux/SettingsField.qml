import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    property string label
    property string hint: ""
    property string errorText: ""
    property alias text: entry.text
    property alias placeholderText: entry.placeholderText
    property alias input: entry
    signal edited(string value)
    Layout.fillWidth: true
    Layout.margins: Theme.padding
    spacing: Theme.spaceSmall
    Text {
        Layout.fillWidth: true
        text: root.label
        textFormat: Text.PlainText
        color: entry.activeFocus ? Theme.accent : Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        wrapMode: Text.Wrap
    }
    TextField {
        id: entry
        objectName: root.objectName + "Input"
        Layout.fillWidth: true
        implicitHeight: 30
        padding: 0
        color: Theme.text
        placeholderTextColor: Theme.muted
        selectionColor: Theme.textSelection
        selectedTextColor: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize
        selectByMouse: true
        Accessible.name: root.label
        Accessible.description: root.errorText || root.hint
        onTextEdited: root.edited(text)
        background: Rectangle {
            color: "transparent"
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: entry.activeFocus || root.errorText ? 2 : 1
                color: root.errorText ? Theme.danger : entry.activeFocus ? Theme.accent : Theme.outline
                opacity: entry.activeFocus || root.errorText ? 1 : 0
            }
        }
    }
    Text {
        Layout.fillWidth: true
        visible: text !== ""
        text: root.errorText || root.hint
        textFormat: Text.PlainText
        color: root.errorText ? Theme.danger : Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        wrapMode: Text.Wrap
    }
}
