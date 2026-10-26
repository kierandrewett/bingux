import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import "auth"

// The login greeter on KioskScreen. The card holds three pages: "login" (the
// account, its password prompt and the session link), "accounts" (switch user
// or type a name) and "sessions" (the session list). Back and Escape return
// to the page that opened the current one. One GreeterAuthentication and one
// GreeterCatalog are shared by every output; only the interactive output
// takes input and plays the hand-over.
KioskScreen {
    id: root

    required property var authentication
    required property var catalog
    property string selectedUser: ""
    property bool customUser: false
    property int sessionIndex: 0
    readonly property var selectedSession: catalog.sessions.length > 0 ? catalog.sessions[Math.min(sessionIndex, catalog.sessions.length - 1)] : null
    readonly property var selectedAccount: catalog.users.find(user => user.name === root.selectedUser) ?? null
    readonly property bool multipleAccounts: catalog.users.length > 1 || (catalog.users.length > 0 && customUser)
    // The password field, for the greeter UI test to type into.
    readonly property var responseField: response
    readonly property var pageStack: pages
    readonly property bool ready: catalog.loaded && selectedUser !== "" && selectedSession !== null
    readonly property bool promptOpen: authentication.responseRequired && !authentication.finishing
    readonly property bool canSubmit: ready && interactive && promptOpen
    readonly property string statusText: authentication.status !== "" ? authentication.status : authentication.hint
    readonly property string accountDisplayName: customUser ? "" : selectedAccount ? selectedAccount.displayName : selectedUser
    readonly property url accountImage: !customUser && selectedAccount && selectedAccount.icon ? "file://" + selectedAccount.icon : ""
    // "Other" keeps the wallpaper of the account shown before it, so the
    // screen does not cut to the plain tint while a name is typed.
    readonly property var wallpaperAccount: selectedAccount && selectedAccount.wallpaper ? selectedAccount : catalog.users.find(user => user.name === catalog.rememberedUser && user.wallpaper) ?? null

    wallpaper: wallpaperAccount ? "file://" + wallpaperAccount.wallpaper : ""
    cardTitle: pages.current === "accounts" ? "Choose an account" : pages.current === "sessions" ? "Session" : customUser ? "Sign in" : accountDisplayName
    backVisible: pages.canGoBack
    primaryText: "Log in"
    primaryVisible: pages.current === "login"
    primaryEnabled: canSubmit && response.text.length > 0
    secondaryText: "Switch user"
    secondaryIcon: "avatar-default-symbolic"
    secondaryVisible: pages.current === "login" && multipleAccounts && interactive
    cardBodyHeight: 232

    onPrimaryRequested: submitResponse()
    onSecondaryRequested: pages.go("accounts")
    onBackRequested: {
        if (pages.back())
            focusPrompt();
    }

    function submitResponse() {
        if (!canSubmit)
            return;
        authentication.respond(response.text);
        response.text = "";
    }

    function focusPrompt() {
        if (!interactive)
            return;
        if (pages.current !== "login")
            return;
        if (customUser)
            userName.forceActiveFocus();
        else
            response.forceActiveFocus();
    }

    function startWhenReady() {
        if (ready && interactive && !authentication.active && !authentication.finishing)
            authentication.start();
    }

    function selectUser(name) {
        if (name === selectedUser && !customUser)
            return;
        if (authentication.active || authentication.finishing)
            authentication.cancel();
        customUser = false;
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
        if (authentication.active || authentication.finishing)
            authentication.cancel();
        customUser = true;
        selectedUser = "";
        userName.text = "";
        pages.back();
        userName.forceActiveFocus();
    }

    onSelectedSessionChanged: authentication.session = selectedSession
    onReadyChanged: startWhenReady()

    Component.onCompleted: {
        pages.replace("login");
        if (catalog.loaded)
            chooseInitialUser();
        focusPrompt();
        startWhenReady();
    }
    Connections {
        target: root.catalog
        function onReady() {
            root.chooseInitialUser();
        }
    }
    Connections {
        target: root.authentication
        function onResponseRequested() {
            response.text = "";
            root.focusPrompt();
        }
        // Credentials were accepted: the interactive output plays the exit,
        // then hands over to the chosen session. finish() is idempotent.
        function onFinishingChanged() {
            if (root.authentication.finishing && root.interactive)
                exit.start();
        }
    }
    Timer {
        id: exit
        interval: Theme.sessionExitMotion
        onTriggered: root.authentication.finish()
    }

    KioskPageStack {
        id: pages
        objectName: "greeterPages"
        anchors.fill: parent

        // The login page: the account, its prompt and the session link.
        KioskPage {
            stack: pages
            pageId: "login"
            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.gap

                // "Other": a neutral avatar and a username field take the
                // place of the account picture and name.
                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 84
                    visible: root.customUser
                    TextField {
                        id: userName
                        objectName: "greeterUserName"
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.verticalCenter: parent.verticalCenter
                        width: 240
                        implicitHeight: 36
                        enabled: !root.authentication.finishing
                        placeholderText: "Username"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall + 1
                        color: Theme.text
                        placeholderTextColor: Theme.muted
                        selectByMouse: true
                        leftPadding: Theme.padding
                        Accessible.name: placeholderText
                        background: Rectangle {
                            radius: height / 2
                            color: Theme.elevated
                            border.width: 1
                            border.color: userName.activeFocus ? Theme.accent : Qt.rgba(1, 1, 1, 0.12)
                        }
                        onAccepted: {
                            const name = text.trim();
                            if (name !== "")
                                root.selectUser(name);
                        }
                    }
                }
                UserAvatar {
                    Layout.alignment: Qt.AlignHCenter
                    visible: !root.customUser
                    size: 72
                    source: root.accountImage
                    name: root.accountDisplayName
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignHCenter
                    spacing: 8
                    BusyIndicator {
                        implicitWidth: 18
                        implicitHeight: 18
                        running: root.authentication.finishing || (root.authentication.active && !root.authentication.responseRequired)
                        visible: running
                    }
                    Text {
                        objectName: "greeterStatus"
                        Layout.fillWidth: true
                        text: root.statusText
                        color: root.authentication.statusIsError ? Theme.danger : Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                    }
                }

                TextField {
                    id: response
                    objectName: "greeterResponse"
                    Layout.fillWidth: true
                    Layout.preferredHeight: 36
                    visible: root.interactive
                    enabled: root.canSubmit
                    opacity: root.canSubmit ? 1 : 0.55
                    placeholderText: root.promptOpen && root.authentication.prompt !== "" ? root.authentication.prompt : "Password"
                    echoMode: root.authentication.responseVisible ? TextInput.Normal : TextInput.Password
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall + 1
                    color: Theme.text
                    placeholderTextColor: Theme.muted
                    selectByMouse: false
                    leftPadding: Theme.padding
                    rightPadding: Theme.padding
                    Accessible.name: placeholderText
                    background: Rectangle {
                        radius: height / 2
                        color: Theme.elevated
                        border.width: 1
                        border.color: response.activeFocus ? Theme.accent : Qt.rgba(1, 1, 1, 0.12)
                    }
                    onAccepted: root.submitResponse()
                    // A rejected attempt shakes the field.
                    transform: Translate {
                        id: shakeOffset
                    }
                    Connections {
                        target: root.authentication
                        function onStatusIsErrorChanged() {
                            if (root.authentication.statusIsError)
                                shake.restart();
                        }
                    }
                    SequentialAnimation {
                        id: shake
                        loops: 1
                        NumberAnimation {
                            target: shakeOffset
                            property: "x"
                            to: -8
                            duration: Theme.motion / 2
                        }
                        NumberAnimation {
                            target: shakeOffset
                            property: "x"
                            to: 8
                            duration: Theme.motion / 2
                        }
                        NumberAnimation {
                            target: shakeOffset
                            property: "x"
                            to: 0
                            duration: Theme.motion / 2
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignHCenter
                    spacing: Theme.gap
                    SessionLink {
                        objectName: "sessionRetry"
                        visible: root.interactive && root.ready && !root.authentication.active && !root.authentication.responseRequired && !root.authentication.finishing
                        text: "Try again"
                        onClicked: root.authentication.start()
                    }
                    SessionLink {
                        objectName: "greeterSession"
                        visible: root.catalog.sessions.length > 1
                        iconName: "emblem-system-symbolic"
                        text: root.selectedSession ? root.selectedSession.name : "Session"
                        enabled: !root.authentication.finishing
                        Accessible.name: "Session: " + (root.selectedSession ? root.selectedSession.name : "none")
                        onClicked: pages.go("sessions")
                    }
                }
            }
        }

        // Switch user: the known accounts and "Other…".
        KioskPage {
            stack: pages
            pageId: "accounts"
            ColumnLayout {
                anchors.fill: parent
                spacing: 4
                objectName: "greeterAccountList"
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
                            root.selectUser(modelData.name);
                            pages.back();
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
                Item {
                    Layout.fillHeight: true
                }
            }
        }

        // Session choice: the sessions the greetd catalog found.
        KioskPage {
            stack: pages
            pageId: "sessions"
            ColumnLayout {
                anchors.fill: parent
                spacing: 4
                objectName: "greeterSessionList"
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
                            pages.back();
                            root.focusPrompt();
                        }
                    }
                }
                Item {
                    Layout.fillHeight: true
                }
            }
        }
    }
}
