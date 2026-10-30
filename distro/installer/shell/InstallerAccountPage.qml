import QtQuick
import "InstallerSchema.js" as Schema

// The first user: name, username, password, avatar, automatic log in, and the
// optional root password. The plan carries root_password and autologin. Two
// columns; the root password row appears only while its switch is on.
InstallerPage {
    id: page

    title: "Your account"
    fullBody: true
    bodyHeight: 600

    property string passwordConfirm: ""
    property string rootConfirm: ""

    readonly property real us: InstallerState.uiScale
    readonly property real half: Math.round((width - Theme.gap * 2) / 2)

    readonly property string nameError: Schema.validFullName(InstallerState.fullName)
    readonly property string usernameError: InstallerState.username === "" ? "" : Schema.validUsername(InstallerState.username)
    readonly property var strength: Schema.passwordStrength(InstallerState.password)
    readonly property string passwordError: Schema.validPassword(InstallerState.password)
    readonly property string confirmError: passwordConfirm !== InstallerState.password ? "The passwords do not match." : ""
    readonly property string rootError: InstallerState.rootPasswordSet ? Schema.validPassword(InstallerState.rootPassword) : ""
    readonly property string rootConfirmError: InstallerState.rootPasswordSet && rootConfirm !== InstallerState.rootPassword
        ? "The passwords do not match." : ""
    readonly property color meterColor: strength.score <= 1 ? Theme.danger : strength.score === 2 ? Theme.warning : Theme.success

    primaryEnabled: InstallerState.username !== "" && usernameError === "" && nameError === ""
        && passwordError === "" && confirmError === ""
        && rootError === "" && rootConfirmError === ""

    function primary() {
        page.wizard.go("summary");
    }

    InstallerScroll {
        spacing: Math.round(16 * page.us)

        Row {
            width: parent.width
            spacing: Theme.gap * 2

            // Left: identity and password.
            Column {
                id: left
                width: page.half
                spacing: Math.round(12 * page.us)

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
                    message: InstallerState.password === "" ? "" : page.passwordError
                    onEdited: InstallerState.password = text
                }
                Row {
                    width: parent.width
                    spacing: 6
                    visible: InstallerState.password !== ""
                    Repeater {
                        model: [0, 1, 2, 3]
                        Rectangle {
                            required property int modelData
                            width: (parent.width - 18) / 4
                            height: Math.round(8 * page.us)
                            radius: height / 2
                            color: modelData < page.strength.score ? page.meterColor : Qt.rgba(1, 1, 1, 0.12)
                        }
                    }
                }
                Text {
                    visible: InstallerState.password !== ""
                    width: parent.width
                    text: "Password strength: " + page.strength.label
                    color: page.meterColor
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(16 * page.us)
                    font.weight: Font.DemiBold
                }
            }

            // Right: confirmation, picture, and the two switches.
            Column {
                width: page.half
                spacing: Math.round(12 * page.us)

                InstallerField {
                    caption: "Confirm password"
                    echoMode: TextInput.Password
                    text: page.passwordConfirm
                    invalid: page.passwordConfirm !== "" && page.confirmError !== ""
                    message: page.passwordConfirm === "" ? "" : page.confirmError
                    onEdited: page.passwordConfirm = text
                }

                Text {
                    width: parent.width
                    text: "Picture (shown on this screen only)"
                    color: Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(15 * page.us)
                }
                Row {
                    spacing: Math.round(12 * page.us)
                    Repeater {
                        model: InstallerState.avatarChoices
                        Rectangle {
                            required property string modelData
                            readonly property bool chosen: InstallerState.avatar === modelData
                            width: Math.round(64 * page.us)
                            height: width
                            radius: width / 2
                            color: chosen ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.18) : Theme.elevated
                            border.width: chosen ? 3 : 1
                            border.color: chosen ? Theme.accent : Qt.rgba(1, 1, 1, 0.16)
                            Image {
                                anchors.fill: parent
                                anchors.margins: Math.round(6 * page.us)
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
                        : "Starts " + InstallerState.username + "'s desktop at boot."
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
            }
        }

        // Root password fields, only while the root switch is on.
        Row {
            width: parent.width
            spacing: Theme.gap * 2
            visible: InstallerState.rootPasswordSet

            InstallerField {
                width: page.half
                caption: "Root password"
                echoMode: TextInput.Password
                placeholderText: "At least 8 characters"
                text: InstallerState.rootPassword
                invalid: InstallerState.rootPassword !== "" && page.rootError !== ""
                message: InstallerState.rootPassword === "" ? "" : page.rootError
                onEdited: InstallerState.rootPassword = text
            }
            InstallerField {
                width: page.half
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
