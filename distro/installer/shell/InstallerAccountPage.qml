import QtQuick
import "InstallerSchema.js" as Schema

// The first user: name, username, password, avatar, automatic log in, and the
// optional root password. The plan carries root_password and autologin.
InstallerPage {
    id: page

    title: "Your account"
    bodyHeight: 600

    property string passwordConfirm: ""
    property string rootConfirm: ""

    readonly property string nameError: Schema.validFullName(InstallerState.fullName)
    readonly property string usernameError: InstallerState.username === "" ? "" : Schema.validUsername(InstallerState.username)
    readonly property var strength: Schema.passwordStrength(InstallerState.password)
    readonly property string passwordError: Schema.validPassword(InstallerState.password)
    readonly property string confirmError: passwordConfirm !== InstallerState.password ? "The passwords do not match." : ""
    readonly property string rootError: InstallerState.rootPasswordSet ? Schema.validPassword(InstallerState.rootPassword) : ""
    readonly property string rootConfirmError: InstallerState.rootPasswordSet && rootConfirm !== InstallerState.rootPassword
        ? "The passwords do not match." : ""

    primaryEnabled: InstallerState.username !== "" && usernameError === "" && nameError === ""
        && passwordError === "" && confirmError === ""
        && rootError === "" && rootConfirmError === ""

    function primary() {
        page.wizard.go("summary");
    }

    // The root password fields make the form taller than the card body; scroll rather than overflow the buttons.
    Flickable {
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: column.height

        Column {
            id: column
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
            }

            InstallerText {
                tone: "text"
                text: "Picture (shown on this screen only; the install does not use it yet)"
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

            InstallerSwitchRow {
                label: "Log in automatically"
                note: InstallerState.username === "" ? "Enter a username first."
                    : "Starts " + InstallerState.username + "'s desktop at boot. The login screen still appears when you log out."
                enabled: InstallerState.username !== ""
                checked: InstallerState.autoLogin && InstallerState.username !== ""
                onToggled: value => InstallerState.autoLogin = value
            }
            InstallerSwitchRow {
                label: "Set a root password"
                note: "Leave it off to lock the root account."
                checked: InstallerState.rootPasswordSet
                onToggled: value => InstallerState.rootPasswordSet = value
            }
            InstallerField {
                visible: InstallerState.rootPasswordSet
                caption: "Root password"
                echoMode: TextInput.Password
                placeholderText: "At least 8 characters"
                text: InstallerState.rootPassword
                invalid: InstallerState.rootPassword !== "" && page.rootError !== ""
                message: InstallerState.rootPassword === "" ? "" : page.rootError
                onEdited: InstallerState.rootPassword = text
            }
            InstallerField {
                visible: InstallerState.rootPasswordSet
                caption: "Confirm root password"
                echoMode: TextInput.Password
                text: page.rootConfirm
                invalid: page.rootConfirm !== "" && page.rootConfirmError !== ""
                message: page.rootConfirm === "" ? "" : page.rootConfirmError
                onEdited: page.rootConfirm = text
            }
        }
    }
}
