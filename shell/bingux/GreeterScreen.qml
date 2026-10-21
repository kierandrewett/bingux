import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

// The login greeter: SessionScreen plus the account and session choice. One
// GreeterAuthentication and one GreeterCatalog are shared by every output;
// only the interactive output shows the controls.
//
// The selected account is front and centre with its password prompt, like a
// lock screen. Other accounts sit in the bottom-left corner as a small stack
// that opens a list; the session choice is a quiet icon in the bottom-right.
// Power actions live in the session bar's control centre, not here.
SessionScreen {
    id: root

    required property var catalog
    property string selectedUser: ""
    property bool customUser: false
    property int sessionIndex: 0
    property bool accountsOpen: false
    property bool sessionsOpen: false
    readonly property var selectedSession: catalog.sessions.length > 0 ? catalog.sessions[Math.min(sessionIndex, catalog.sessions.length - 1)] : null
    readonly property var selectedAccount: catalog.users.find(user => user.name === root.selectedUser) ?? null
    readonly property var otherAccounts: catalog.users.filter(user => user.name !== root.selectedUser || root.customUser)
    readonly property bool multipleAccounts: catalog.users.length > 1 || (catalog.users.length > 0 && customUser)

    submitLabel: "Log in"
    accountName: selectedUser
    accountDisplayName: customUser ? "" : selectedAccount ? selectedAccount.displayName : selectedUser
    accountImage: !customUser && selectedAccount && selectedAccount.icon ? "file://" + selectedAccount.icon : ""
    // "Other" keeps the wallpaper of the account that was shown before it,
    // so the screen does not cut to the plain tint while a name is typed.
    readonly property var wallpaperAccount: selectedAccount && selectedAccount.wallpaper ? selectedAccount : catalog.users.find(user => user.name === catalog.rememberedUser && user.wallpaper) ?? null
    wallpaper: wallpaperAccount ? "file://" + wallpaperAccount.wallpaper : ""
    showAccount: !customUser
    ready: catalog.loaded && selectedUser !== "" && selectedSession !== null

    function selectUser(name) {
        accountsOpen = false;
        if (name === selectedUser && !customUser)
            return;
        if (authentication.active || authentication.finishing)
            authentication.cancel();
        selectedUser = name;
        authentication.userName = name;
        authentication.start();
        focusPrompt();
    }

    function chooseInitialUser() {
        const names = catalog.users.map(user => user.name);
        if (catalog.rememberedUser !== "" && names.includes(catalog.rememberedUser))
            selectUser(catalog.rememberedUser);
        else if (names.length > 0)
            selectUser(names[0]);
        else
            customUser = true;
        const sessionIds = catalog.sessions.map(session => session.id);
        const remembered = sessionIds.indexOf(catalog.rememberedSession);
        sessionIndex = remembered >= 0 ? remembered : 0;
    }

    function openCustomUser() {
        accountsOpen = false;
        if (authentication.active || authentication.finishing)
            authentication.cancel();
        customUser = true;
        selectedUser = "";
        userName.text = "";
        userName.forceActiveFocus();
    }

    onSelectedSessionChanged: authentication.session = selectedSession

    Connections {
        target: root.catalog
        function onReady() {
            root.chooseInitialUser();
        }
    }
    Component.onCompleted: if (catalog.loaded)
        chooseInitialUser()

    // Escape or a click outside closes an open list.
    Keys.onEscapePressed: event => {
        if (root.accountsOpen || root.sessionsOpen) {
            root.accountsOpen = false;
            root.sessionsOpen = false;
            event.accepted = true;
        }
    }
    MouseArea {
        anchors.fill: parent
        z: 5
        visible: root.accountsOpen || root.sessionsOpen
        onClicked: {
            root.accountsOpen = false;
            root.sessionsOpen = false;
            root.focusPrompt();
        }
    }

    leadingContent: ColumnLayout {
        width: parent.width
        spacing: Theme.gap
        visible: root.interactive && root.customUser

        // "Other" account: a neutral avatar and a username field take the
        // place of the account picture and name.
        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            width: 72
            height: 72
            radius: 36
            color: root.glass
            border.width: 1
            border.color: root.glassOutline
            SymbolicIcon {
                anchors.centerIn: parent
                implicitSize: 28
                source: AdwaitaIcons.source("avatar-default-symbolic")
                color: root.ink
            }
        }
        TextField {
            id: userName
            objectName: "greeterUserName"
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 200
            implicitHeight: 32
            enabled: !root.authentication.finishing
            placeholderText: "Username"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall + 1
            color: root.ink
            placeholderTextColor: root.inkMuted
            selectByMouse: true
            leftPadding: Theme.padding
            Accessible.name: placeholderText
            background: SessionGlass {
                backdrop: root.backdrop
                outline: userName.activeFocus ? Qt.rgba(1, 1, 1, 0.6) : root.glassOutline
            }
            onAccepted: {
                const name = text.trim();
                if (name !== "")
                    root.selectUser(name);
            }
        }
    }

    // Bottom left: the other accounts as a small stack of avatars. Clicking
    // opens the full list with "Other…" for an account that is not listed.
    cornerLeadingContent: Item {
        id: accountSwitch
        objectName: "greeterAccounts"
        visible: root.multipleAccounts
        implicitWidth: switchButton.implicitWidth
        implicitHeight: switchButton.implicitHeight

        AbstractButton {
            id: switchButton
            objectName: "greeterSwitchUser"
            implicitWidth: switchRow.implicitWidth + 16
            implicitHeight: 36
            hoverEnabled: true
            activeFocusOnTab: true
            enabled: !root.authentication.finishing
            Accessible.name: "Switch user"
            onClicked: {
                root.sessionsOpen = false;
                root.accountsOpen = !root.accountsOpen;
            }
            background: Rectangle {
                radius: height / 2
                color: switchButton.down ? Qt.rgba(1, 1, 1, 0.3) : switchButton.hovered || root.accountsOpen ? Qt.rgba(1, 1, 1, 0.18) : Qt.rgba(1, 1, 1, 0.08)
                border.width: 1
                border.color: switchButton.visualFocus ? Qt.rgba(1, 1, 1, 0.5) : Qt.rgba(1, 1, 1, 0.12)
                Behavior on color {
                    ColorAnimation {
                        duration: Theme.motion
                    }
                }
            }
            contentItem: Item {
                Row {
                    id: switchRow
                    anchors.centerIn: parent
                    spacing: 8
                    // Up to three avatars, overlapping like a stack.
                    Item {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(26, 26 + (Math.min(root.otherAccounts.length, 3) - 1) * 18)
                        height: 26
                        Repeater {
                            model: root.otherAccounts.slice(0, 3)
                            SessionAvatar {
                                required property var modelData
                                required property int index
                                x: index * 18
                                z: -index
                                size: 26
                                source: modelData.icon ? "file://" + modelData.icon : ""
                                name: modelData.displayName
                            }
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.customUser ? "Accounts" : "Switch user"
                        color: root.ink
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        font.weight: Font.Medium
                    }
                }
            }
        }

        SessionPopover {
            objectName: "greeterAccountList"
            anchors.left: parent.left
            anchors.bottom: parent.top
            anchors.bottomMargin: Theme.gap
            open: root.accountsOpen
            backdrop: root.backdrop
            width: 260
            ColumnLayout {
                width: parent.width
                spacing: 2
                Repeater {
                    model: root.catalog.users
                    SessionRow {
                        required property var modelData
                        objectName: "greeterUser-" + modelData.name
                        Layout.fillWidth: true
                        text: modelData.displayName
                        selected: modelData.name === root.selectedUser && !root.customUser
                        enabled: !root.authentication.finishing
                        avatarSource: modelData.icon ? "file://" + modelData.icon : ""
                        avatarName: modelData.displayName
                        onClicked: {
                            root.customUser = false;
                            root.selectUser(modelData.name);
                        }
                    }
                }
                SessionRow {
                    objectName: "greeterOtherUser"
                    Layout.fillWidth: true
                    text: "Other…"
                    iconName: "avatar-default-symbolic"
                    selected: root.customUser
                    enabled: !root.authentication.finishing
                    onClicked: root.openCustomUser()
                }
            }
        }
    }

    // Bottom right: the session choice. One session shows nothing; more than
    // one shows a quiet icon that opens the list with the current one ticked.
    cornerContent: Item {
        objectName: "greeterSessions"
        visible: root.catalog.sessions.length > 1
        implicitWidth: sessionButton.implicitWidth
        implicitHeight: sessionButton.implicitHeight

        SessionLink {
            id: sessionButton
            objectName: "greeterSession"
            iconName: "emblem-system-symbolic"
            dimmed: !root.sessionsOpen
            Accessible.name: "Session: " + (root.selectedSession ? root.selectedSession.name : "none")
            enabled: !root.authentication.finishing
            onClicked: {
                root.accountsOpen = false;
                root.sessionsOpen = !root.sessionsOpen;
            }
        }

        SessionPopover {
            objectName: "greeterSessionList"
            anchors.right: parent.right
            anchors.bottom: parent.top
            anchors.bottomMargin: Theme.gap
            open: root.sessionsOpen
            backdrop: root.backdrop
            width: 220
            ColumnLayout {
                width: parent.width
                spacing: 2
                Text {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.bottomMargin: 4
                    text: "Session"
                    color: root.inkMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall - 1
                    font.weight: Font.DemiBold
                    font.capitalization: Font.AllUppercase
                    font.letterSpacing: 0.6
                }
                Repeater {
                    model: root.catalog.sessions
                    SessionRow {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        text: modelData.name
                        selected: index === root.sessionIndex
                        ticked: selected
                        enabled: !root.authentication.finishing
                        onClicked: {
                            root.sessionIndex = index;
                            root.sessionsOpen = false;
                            root.focusPrompt();
                        }
                    }
                }
            }
        }
    }
}
