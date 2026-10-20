import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// The login greeter: SessionScreen plus account choice, session choice and
// power actions. One GreeterAuthentication and one GreeterCatalog are shared
// by every output; only the interactive output shows the controls.
SessionScreen {
    id: root

    required property var catalog
    property string selectedUser: ""
    property bool customUser: false
    property int sessionIndex: 0
    readonly property var selectedSession: catalog.sessions.length > 0 ? catalog.sessions[Math.min(sessionIndex, catalog.sessions.length - 1)] : null
    readonly property var selectedAccount: catalog.users.find(user => user.name === root.selectedUser) ?? null
    readonly property bool powerBusy: power.running

    submitLabel: "Log in"
    accountName: selectedUser
    accountDisplayName: selectedAccount ? selectedAccount.displayName : selectedUser
    accountImage: selectedAccount && selectedAccount.icon ? "file://" + selectedAccount.icon : ""
    wallpaper: selectedAccount && selectedAccount.wallpaper ? "file://" + selectedAccount.wallpaper : ""
    ready: catalog.loaded && selectedUser !== "" && selectedSession !== null

    function selectUser(name) {
        if (name === selectedUser)
            return;
        if (authentication.active || authentication.finishing)
            authentication.cancel();
        selectedUser = name;
        authentication.userName = name;
        authentication.start();
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

    function nextSession() {
        if (catalog.sessions.length > 1)
            sessionIndex = (sessionIndex + 1) % catalog.sessions.length;
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

    Process {
        id: power
        onExited: (code, _status) => {
            if (code !== 0) {
                root.authentication.status = "The power action was refused";
                root.authentication.statusIsError = true;
            }
        }
    }

    leadingContent: ColumnLayout {
        width: parent.width
        spacing: Theme.gap
        visible: root.interactive

        Flow {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignHCenter
            spacing: Theme.spaceSmall
            visible: root.catalog.users.length > 1
            Repeater {
                model: root.catalog.users
                ActionButton {
                    required property var modelData
                    objectName: "greeterUser-" + modelData.name
                    flat: modelData.name !== root.selectedUser
                    text: modelData.displayName
                    enabled: !root.authentication.finishing
                    onClicked: {
                        root.customUser = false;
                        root.selectUser(modelData.name);
                    }
                }
            }
            ActionButton {
                objectName: "greeterOtherUser"
                flat: !root.customUser
                text: "Other…"
                enabled: !root.authentication.finishing
                onClicked: {
                    root.customUser = true;
                    userName.forceActiveFocus();
                }
            }
        }

        TextField {
            id: userName
            objectName: "greeterUserName"
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 220
            implicitHeight: Theme.controlHeight
            visible: root.customUser
            enabled: !root.authentication.finishing
            placeholderText: "Username"
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            color: Theme.text
            placeholderTextColor: Theme.muted
            horizontalAlignment: TextInput.AlignHCenter
            selectByMouse: true
            Accessible.name: placeholderText
            background: Rectangle {
                radius: height / 2
                color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.14), Theme.background)
                border.width: 1
                border.color: userName.activeFocus ? Theme.accent : Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.18), Theme.background)
            }
            onAccepted: {
                const name = text.trim();
                if (name !== "")
                    root.selectUser(name);
            }
        }
    }

    trailingContent: RowLayout {
        width: parent.width
        spacing: Theme.gap
        visible: root.interactive
        Item {
            Layout.fillWidth: true
        }
        ActionButton {
            objectName: "greeterSession"
            flat: true
            iconName: "emblem-system-symbolic"
            trailingIconName: root.catalog.sessions.length > 1 ? "pan-down-symbolic" : ""
            text: root.selectedSession ? root.selectedSession.name : "No sessions"
            enabled: root.catalog.sessions.length > 1 && !root.authentication.finishing
            onClicked: root.nextSession()
        }
        IconButton {
            objectName: "greeterRestart"
            iconName: "system-reboot-symbolic"
            label: "Restart"
            enabled: !root.powerBusy && !root.authentication.finishing
            onClicked: {
                power.command = ["systemctl", "reboot"];
                power.running = true;
            }
        }
        IconButton {
            objectName: "greeterShutDown"
            iconName: "system-shutdown-symbolic"
            label: "Shut Down"
            enabled: !root.powerBusy && !root.authentication.finishing
            onClicked: {
                power.command = ["systemctl", "poweroff"];
                power.running = true;
            }
        }
        Item {
            Layout.fillWidth: true
        }
    }
}
