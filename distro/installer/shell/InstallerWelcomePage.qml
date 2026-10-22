import QtQuick
import Quickshell

// First page: the language. Choosing one also moves the keyboard and time
// zone, which the following pages show and may change. "Try Bingux" leaves
// the installer for the live desktop without installing anything.
InstallerPage {
    id: page

    title: "Welcome to Bingux"
    allowBack: false
    bodyHeight: 470
    secondaryText: "Try Bingux"
    secondaryVisible: true

    function primary() {
        page.wizard.go("keyboard");
    }

    function secondary() {
        Quickshell.execDetached(["bingux-live-switch", "live"]);
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
