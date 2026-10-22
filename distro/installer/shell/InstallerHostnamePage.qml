import QtQuick
import "InstallerSchema.js" as Schema

// The computer name, checked against the RFC 1123 label rule in the plan schema.
InstallerPage {
    id: page

    title: "Computer name"
    bodyHeight: 200
    primaryEnabled: hostnameError === ""

    readonly property string hostnameError: Schema.validHostname(InstallerState.hostname)

    function primary() {
        page.wizard.go("disk");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            text: "The name shows on the network and in the terminal. Letters, digits and hyphens."
        }
        InstallerField {
            caption: "Computer name"
            placeholderText: "bingux"
            text: InstallerState.hostname
            invalid: page.hostnameError !== ""
            message: page.hostnameError
            maximumLength: 63
            onEdited: InstallerState.hostname = text
            onAccepted: page.primary()
        }
    }
}
