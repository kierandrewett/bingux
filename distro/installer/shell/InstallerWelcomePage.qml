import QtQuick
import Quickshell

// First page: the language. Choosing one also moves the keyboard and time
// zone, which the following pages show and may change. "Try Bingux" leaves
// the installer for the live desktop without installing anything.
InstallerPage {
    id: page

    title: "Welcome to Bingux"
    lead: "Choose a language for the installer and the installed system. The keyboard and time zone follow it."
    allowBack: false
    bodyHeight: 470
    secondaryText: "Try Bingux"
    secondaryVisible: true

    // The rows fill the card height, at most 64 px (scaled) each.
    readonly property int rowGap: 4
    readonly property real rowHeight: Math.max(40, Math.min(Math.round(64 * InstallerState.uiScale),
        Math.floor((height - (InstallerState.localeChoices.length - 1) * rowGap) / InstallerState.localeChoices.length)))

    function primary() {
        page.wizard.go("keyboard");
    }

    function secondary() {
        Quickshell.execDetached(["bingux-live-switch", "live"]);
    }

    Column {
        width: parent.width
        spacing: page.rowGap

        Repeater {
            model: InstallerState.localeChoices
            Item {
                id: entry
                required property var modelData
                readonly property bool selected: InstallerState.locale === modelData.locale
                width: parent.width
                height: page.rowHeight

                InstallerOption {
                    width: entry.width
                    rowHeight: page.rowHeight
                    text: entry.modelData.name
                    selected: entry.selected
                    ticked: entry.selected
                    onClicked: InstallerState.chooseLocale(entry.modelData)
                }
                Rectangle {
                    visible: entry.selected
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.round(5 * InstallerState.uiScale)
                    height: parent.height * 0.6
                    radius: width / 2
                    color: Theme.accent
                }
            }
        }
    }
}
