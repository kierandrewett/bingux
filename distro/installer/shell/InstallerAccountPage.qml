import QtQuick
import "InstallerSchema.js" as Schema

// The first user: name, username, password and avatar. Auto login and the root
// password are recorded for the summary but the protocol does not carry them yet.
InstallerPage {
    id: page

    title: "Your account"
    bodyHeight: 620

    property string passwordConfirm: ""

    readonly property string nameError: Schema.validFullName(InstallerState.fullName)
    readonly property string usernameError: InstallerState.username === "" ? "" : Schema.validUsername(InstallerState.username)
    readonly property var strength: Schema.passwordStrength(InstallerState.password)
    readonly property string passwordError: Schema.validPassword(InstallerState.password)
    readonly property string confirmError: passwordConfirm !== InstallerState.password ? "The passwords do not match." : ""

    primaryEnabled: InstallerState.username !== "" && usernameError === "" && nameError === ""
        && passwordError === "" && confirmError === ""

    function primary() {
        page.wizard.go("summary");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerField {
            caption: "Full name"
            placeholderText: "Ada Lovelace"
            text: InstallerState.fullName
            invalid: page.nameError !== ""
            message: page.nameError
            maximumLength: 128
            onEdited: {
                InstallerState.fullName = text;
                if (!InstallerState.usernameEdited)
                    InstallerState.username = Schema.deriveUsername(text);
            }
        }
        InstallerField {
            caption: "Username"
            placeholderText: "ada"
            text: InstallerState.username
            invalid: page.usernameError !== ""
            message: page.usernameError
            maximumLength: 32
            onEdited: {
                InstallerState.username = text;
                InstallerState.usernameEdited = true;
            }
        }
        InstallerField {
            caption: "Password"
            echoMode: TextInput.Password
            placeholderText: "At least 8 characters"
            text: InstallerState.password
            invalid: InstallerState.password !== "" && page.passwordError !== ""
            message: InstallerState.password === "" ? "" : page.passwordError !== "" ? page.passwordError
                : "Strength: " + page.strength.label
            onEdited: InstallerState.password = text
        }
        InstallerField {
            caption: "Confirm password"
            echoMode: TextInput.Password
            text: page.passwordConfirm
            invalid: page.passwordConfirm !== "" && page.confirmError !== ""
            message: page.passwordConfirm === "" ? "" : page.confirmError
            onEdited: page.passwordConfirm = text
            onAccepted: page.primary()
        }

        InstallerText {
            tone: "text"
            text: "Picture"
        }
        Row {
            spacing: Theme.gap
            Repeater {
                model: InstallerState.avatarChoices
                Rectangle {
                    required property string modelData
                    readonly property bool chosen: InstallerState.avatar === modelData
                    width: 52
                    height: 52
                    radius: width / 2
                    color: "transparent"
                    border.width: chosen ? 3 : 1
                    border.color: chosen ? Theme.accent : Qt.rgba(1, 1, 1, 0.16)
                    Image {
                        anchors.fill: parent
                        anchors.margins: 3
                        source: Qt.resolvedUrl("assets/" + modelData + ".svg")
                        fillMode: Image.PreserveAspectFit
                        mipmap: true
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: InstallerState.avatar = modelData
                    }
                }
            }
        }
        InstallerText {
            tone: "muted"
            text: "Recorded; the installer does not apply it yet."
        }

        InstallerSwitchRow {
            label: "Log in automatically"
            note: "Recorded, but the installer does not apply it yet."
            checked: InstallerState.autoLogin
            onToggled: value => InstallerState.autoLogin = value
        }
        InstallerSwitchRow {
            label: "Set a root password"
            note: "Recorded, but the installer does not apply it yet. The root account is locked otherwise."
            checked: InstallerState.rootPassword
            onToggled: value => InstallerState.rootPassword = value
        }
    }
}
