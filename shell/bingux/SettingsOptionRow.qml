import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

SettingsRow {
    id: root
    property var options: []
    property string value: ""
    signal chosen(string value)
    valueText: options.find(option => option.value === value)?.label || value
    navigation: true
    navigationRotation: 90
    Accessible.role: Accessible.ComboBox
    onClicked: choices.open()
    Popup {
        id: choices
        x: 8
        y: root.height - 4
        width: root.width - 16
        padding: 4
        focus: true
        onOpened: {
            const index = Math.max(0, root.options.findIndex(option => option.value === root.value));
            optionItems.itemAt(index)?.forceActiveFocus(Qt.PopupFocusReason);
        }
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 10
            color: Theme.settingsSurface
            border.width: 1
            border.color: Theme.outline
        }
        contentItem: ColumnLayout {
            spacing: 0
            Repeater {
                id: optionItems
                model: root.options
                SettingsRow {
                    required property var modelData
                    required property int index
                    title: modelData.label
                    subtitle: modelData.description || ""
                    selected: modelData.value === root.value
                    Keys.onPressed: event => {
                        if (event.key !== Qt.Key_Up && event.key !== Qt.Key_Down)
                            return;
                        optionItems.itemAt(Math.max(0, Math.min(optionItems.count - 1, index + (event.key === Qt.Key_Down ? 1 : -1)))).forceActiveFocus(Qt.TabFocusReason);
                        event.accepted = true;
                    }
                    onClicked: {
                        root.chosen(modelData.value);
                        choices.close();
                        root.forceActiveFocus();
                    }
                }
            }
        }
    }
}
