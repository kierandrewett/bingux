import QtQuick

// Keyboard layout for the installed system, with a field to try it out.
InstallerPage {
    id: page

    title: "Keyboard"
    lead: "Choose the layout for the installed system. Type in the box to check it."
    bodyHeight: 470

    readonly property real ui: InstallerState.uiScale

    function primary() {
        page.wizard.go("network");
    }

    Column {
        width: parent.width
        spacing: Math.round(14 * page.ui)

        Text {
            width: parent.width
            text: "Layout"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(15 * page.ui)
        }
        Grid {
            width: parent.width
            columns: 2
            spacing: Theme.gap
            Repeater {
                model: InstallerState.keymapChoices
                InstallerOption {
                    required property var modelData
                    width: (parent.width - Theme.gap) / 2
                    rowHeight: Math.round(60 * page.ui)
                    text: modelData.name
                    selected: InstallerState.keymap === modelData.keymap
                    ticked: selected
                    onClicked: InstallerState.keymap = modelData.keymap
                }
            }
        }
        InstallerField {
            caption: "Try the layout"
            placeholderText: "Type here"
            fieldHeight: Math.round(60 * page.ui)
            textField.font.pixelSize: Math.round(24 * page.ui)
        }
    }
}
