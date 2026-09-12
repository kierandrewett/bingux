import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root
    property string label
    property var choices: []
    property var values: choices
    property string value
    signal chosen(string value)
    Layout.fillWidth: true
    spacing: 6
    Text {
        text: root.label
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    Flow {
        Layout.fillWidth: true
        spacing: 4
        Repeater {
            model: root.choices
            ActionButton {
                required property string modelData
                required property int index
                text: modelData
                Accessible.role: Accessible.RadioButton
                Accessible.checked: root.value === root.values[index]
                background: ControlCentreButtonSurface {
                    control: parent
                    selected: root.value === root.values[index]
                }
                onClicked: root.chosen(root.values[index])
            }
        }
    }
}
