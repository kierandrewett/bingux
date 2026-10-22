import QtQuick

// Keyboard layout for the installed system, with a field to try it out.
InstallerPage {
    id: page

    title: "Keyboard"
    bodyHeight: 400

    function primary() {
        page.wizard.go("network");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            text: "Choose the layout for the installed system. Type in the box to check it."
        }
        Repeater {
            model: InstallerState.keymapChoices
            SessionRow {
                required property var modelData
                width: parent.width
                text: modelData.name
                selected: InstallerState.keymap === modelData.keymap
                ticked: selected
                onClicked: InstallerState.keymap = modelData.keymap
            }
        }
        InstallerField {
            caption: "Try the layout"
            placeholderText: "Type here"
        }
    }
}
