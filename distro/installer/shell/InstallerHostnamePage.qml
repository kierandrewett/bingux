import QtQuick
import "InstallerSchema.js" as Schema

// The computer name, checked against the RFC 1123 label rule in the plan schema.
InstallerPage {
    id: page

    title: "Computer name"
    lead: "The computer name shows on the network and in the terminal."
    bodyHeight: 200
    primaryEnabled: hostnameError === ""

    readonly property string hostnameError: Schema.validHostname(InstallerState.hostname)

    function primary() {
        page.wizard.go("disk");
    }

    Column {
        width: parent.width
        spacing: Math.round(16 * InstallerState.uiScale)

        InstallerField {
            caption: "Computer name"
            placeholderText: "bingux"
            text: InstallerState.hostname
            invalid: page.hostnameError !== ""
            message: page.hostnameError
            maximumLength: 63
            fieldHeight: Math.round(66 * InstallerState.uiScale)
            textField.font.pixelSize: Math.round(28 * InstallerState.uiScale)
            onEdited: InstallerState.hostname = text
            onAccepted: page.primary()
        }
        InstallerText {
            text: "Use letters, digits and hyphens: 1 to 63 characters. Start and end with a letter or digit."
        }
    }
}
