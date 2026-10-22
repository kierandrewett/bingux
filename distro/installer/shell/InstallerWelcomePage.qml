import QtQuick

// First page: the language. Choosing one also moves the keyboard and time
// zone, which the following pages show and may change.
InstallerPage {
    id: page

    title: "Welcome to Bingux"
    allowBack: false
    bodyHeight: 360

    function primary() {
        page.wizard.go("keyboard");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            text: "Choose a language for the installer and the installed system. The keyboard and time zone follow it."
        }
        Repeater {
            model: InstallerState.localeChoices
            SessionRow {
                required property var modelData
                width: parent.width
                text: modelData.name
                selected: InstallerState.locale === modelData.locale
                ticked: selected
                onClicked: InstallerState.chooseLocale(modelData)
            }
        }
    }
}
