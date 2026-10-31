import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import ".."
import "../auth"

// One output of the bsm greeter. Every output shows the wallpaper, the clock
// and the selected account; the interactive output also takes the password,
// the account picker and the power menu.
//
// The greeter never animates the outer reveal. After a successful sign-in it
// shows a quiet "Signing in" state and keeps its pixels until bsm covers it
// (new login) or slides it away (unlock); `leaving` stays false.
Item {
    id: root

    required property var model
    required property var client
    property bool interactive: true
    readonly property alias screenItem: screen
    readonly property alias picker: picker
    readonly property alias powerMenu: power

    property bool entered: false

    function focusPrompt() {
        if (interactive && !picker.open && !power.open)
            screen.focusPrompt();
    }

    function refreshTheme() {
        const user = root.model.selectedUser;
        authTheme.apply(user !== null ? user.appearance : null);
        // The global policy and the account's own both reduce motion.
        if (root.model.reduceMotion)
            authTheme.reduceMotionRequested = true;
    }

    AuthTheme {
        id: authTheme
    }

    readonly property var appearance: model.selectedUser !== null ? model.selectedUser.appearance : null
    onAppearanceChanged: refreshTheme()
    Connections {
        target: root.model
        function onReduceMotionChanged() {
            root.refreshTheme();
        }
        function onSecretsCleared() {
            screen.clearResponse();
            picker.open = false;
            Qt.callLater(root.focusPrompt);
        }
        function onRejected() {
            screen.reject();
            screen.clearResponse();
            Qt.callLater(root.focusPrompt);
        }
        function onPromptOpenChanged() {
            if (root.model.promptOpen)
                Qt.callLater(root.focusPrompt);
        }
        function onSyncedChanged() {
            root.openPickerWhenNeutral();
        }
        function onSelectedUidChanged() {
            root.openPickerWhenNeutral();
        }
    }

    // With accounts but none selected, the picker is the screen.
    function openPickerWhenNeutral() {
        if (interactive && model.synced && model.selectedUser === null && model.users.length > 0)
            picker.open = true;
    }

    AuthScreen {
        id: screen
        anchors.fill: parent
        theme: authTheme
        interactive: root.interactive
        entered: root.entered
        leaving: false
        dimmed: picker.open
        wallpaper: root.model.wallpaper
        wallpaperRevision: root.model.wallpaperRevision
        cacheWallpaper: true
        displayName: root.model.selectedUser !== null ? root.model.selectedUser.displayName : ""
        avatarSource: root.model.avatar
        showForm: root.model.synced && root.model.selectedUser !== null && !root.model.showProgress
        statusText: root.model.showProgress ? "" : root.model.statusLine
        statusIsError: root.model.statusLineIsError
        promptText: root.model.promptPlaceholder
        echoVisible: root.model.promptKind === "visible"
        canSubmit: root.interactive && root.model.canSubmit
        busy: root.model.busy && root.model.connected && !root.model.showProgress
        spinning: !root.model.quiet
        submitLabel: root.model.intent === "unlock" ? "Unlock" : "Sign in"
        showRetry: root.model.canRetry
        onSubmitted: text => root.client.respond(text)
        onRetryRequested: root.client.retry()

        cardContent: ColumnLayout {
            width: parent ? parent.width : 0
            spacing: Theme.gap
            AuthProgress {
                Layout.fillWidth: true
                theme: authTheme
                backdrop: screen.backdrop
                interactive: root.interactive
                progress: root.model.showProgress ? root.model.progress : null
                onRetryRequested: root.client.retry()
                onBackRequested: {
                    root.client.backToAccounts();
                    if (root.interactive)
                        picker.open = true;
                }
                onPowerRequested: power.open = true
            }
            AuthButton {
                objectName: "operationCancel"
                Layout.alignment: Qt.AlignHCenter
                visible: root.interactive && root.model.operationCancellable
                theme: authTheme
                backdrop: screen.backdrop
                text: "Cancel"
                onClicked: root.client.cancelOperation()
            }
        }

        cornerLeadingContent: [
            AuthButton {
                objectName: "switchUser"
                visible: root.interactive && root.model.canSwitchUser && !root.model.unlockMode
                theme: authTheme
                backdrop: screen.backdrop
                iconName: "avatar-default-symbolic"
                text: "Switch user"
                onClicked: picker.open = true
            },
            AuthButton {
                objectName: "backToOwner"
                visible: root.interactive && root.model.unlockMode && root.model.selectedUid !== root.model.unlockUid && root.model.unlockUser !== null && !root.model.signingIn
                theme: authTheme
                backdrop: screen.backdrop
                iconName: "go-previous-symbolic"
                text: root.model.unlockUser !== null ? "Back to " + root.model.unlockUser.displayName : ""
                onClicked: root.client.selectUser(root.model.unlockUid)
            }
        ]
        cornerContent: [
            BsmNetworkIndicator {
                objectName: "networkStatus"
                active: root.interactive
                theme: authTheme
                backdrop: screen.backdrop
            },
            AuthButton {
                objectName: "powerButton"
                visible: root.interactive && root.model.connected
                theme: authTheme
                backdrop: screen.backdrop
                iconName: "system-shutdown-symbolic"
                Accessible.name: "Power options"
                onClicked: power.open = !power.open
            }
        ]

        overlayContent: Item {
            anchors.fill: parent

            // The subtle switch-user handle on the locked screen, and the
            // swipe-up gesture that opens the same panel.
            Item {
                id: gesture
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 96
                visible: root.interactive && root.model.unlockMode && root.model.canSwitchUser && !picker.open
                DragHandler {
                    id: swipe
                    // translation is back at zero by the time `active` turns
                    // false, so the travel is kept while the drag runs.
                    property real travelled: 0
                    target: null
                    xAxis.enabled: false
                    onTranslationChanged: if (active)
                        travelled = translation.y
                    onActiveChanged: {
                        if (active) {
                            travelled = 0;
                            return;
                        }
                        root.model.gesture = "swipe " + Math.round(travelled);
                        if (travelled < -48)
                            picker.open = true;
                    }
                }
                AbstractButton {
                    id: handle
                    objectName: "switchUserHandle"
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: Theme.gap
                    implicitWidth: handleRow.implicitWidth + 24
                    implicitHeight: 30
                    hoverEnabled: true
                    activeFocusOnTab: true
                    opacity: hovered || visualFocus ? 1 : 0.62
                    Behavior on opacity {
                        NumberAnimation {
                            duration: authTheme.motion
                        }
                    }
                    Accessible.role: Accessible.Button
                    Accessible.name: "Switch user"
                    onClicked: picker.open = true
                    Keys.onReturnPressed: picker.open = true
                    background: Rectangle {
                        radius: height / 2
                        color: handle.down ? Qt.rgba(1, 1, 1, 0.3) : handle.hovered || handle.visualFocus ? Qt.rgba(1, 1, 1, 0.18) : "transparent"
                        border.width: handle.visualFocus ? 1 : 0
                        border.color: Qt.rgba(1, 1, 1, 0.5)
                    }
                    contentItem: Item {
                        Row {
                            id: handleRow
                            anchors.centerIn: parent
                            spacing: 6
                            SymbolicIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                implicitSize: 12
                                source: BinguxIcons.source("pan-up-symbolic")
                                color: authTheme.ink
                            }
                            Text {
                                textFormat: Text.PlainText
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Switch user"
                                color: authTheme.ink
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                        }
                    }
                }
            }

            BsmPowerMenu {
                id: power
                objectName: "powerMenuHost"
                anchors.fill: parent
                theme: authTheme
                backdrop: screen.backdrop
                onRequested: action => {
                    open = false;
                    root.client.power(action);
                }
                onDismissed: {
                    open = false;
                    root.focusPrompt();
                }
            }

            AccountPicker {
                id: picker
                objectName: "accountPicker"
                anchors.fill: parent
                theme: authTheme
                backdrop: screen.backdrop
                users: root.model.pickerUsers
                selectedUid: root.model.selectedUid
                onChosen: uid => {
                    open = false;
                    root.client.selectUser(uid);
                    Qt.callLater(root.focusPrompt);
                }
                onDismissed: {
                    open = false;
                    root.focusPrompt();
                }
            }
        }
    }

    Component.onCompleted: {
        refreshTheme();
        entered = true;
        focusPrompt();
        openPickerWhenNeutral();
    }
}
