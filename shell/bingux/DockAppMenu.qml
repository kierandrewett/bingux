import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.Mpris
import "MediaMatch.js" as MediaMatch

ShellPopup {
    id: appMenu

    required property var appIcon
    required property var button
    required property var dock
    property real dockCentreX: 0
    readonly property bool mediaArtworkAnimating: {
        for (let index = 0; index < mediaControls.count; index++) {
            if (mediaControls.itemAt(index)?.artworkAnimating)
                return true;
        }
        return false;
    }
    readonly property var mediaPlayers: Mpris.players.values.filter(player => MediaMatch.matches(player, button.currentGroup))
    readonly property alias menuEntries: menuColumn

    function layoutCards() {
        notificationPreview.layoutCards();
    }
    function prepareCards() {
        notificationPreview.prepare();
    }
    function updateAnchor() {
        dockCentreX = button.mapToItem(dock.contentItem, button.width / 2, 0).x;
    }

    contentPadding: Theme.gap
    objectName: "dockAppMenu"
    popupHeight: menuColumn.implicitHeight + contentPadding * 2
    popupWidth: mediaPlayers.length > 0 || button.notificationCount > 0 || notificationPreview.renderedNotificationCount > 0 ? 320 : 280
    preferredX: dock.margins.left + dockCentreX - popupWidth / 2
    preferredY: height - dock.dockTopFromBottom - popupHeight - Theme.gap
    revealOriginY: popupHeight
    screen: dock.screen

    Behavior on popupHeight {
        enabled: appMenu.visible && notificationPreview.activeCollapses === 0 && !appMenu.mediaArtworkAnimating

        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 240
            easing.type: Easing.OutCubic
        }
    }
    Behavior on popupWidth {
        enabled: appMenu.visible

        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 240
            easing.type: Easing.OutCubic
        }
    }

    onVisibleChanged: {
        button.menuOpen = visible;
        if (visible) {
            updateAnchor();
            menuNavigation.focusMenu();
        }
    }

    Timer {
        id: closeClickGuard

        interval: 350

        onTriggered: appMenu.dismissOnOutsideClick = true
    }
    MenuNavigator {
        id: menuNavigation

        entries: menuColumn.children
        focusTarget: menuColumn

        onActivateRequested: entry => entry.triggered()
        onEscapeRequested: button.menuOpen = false
    }
    Rectangle {
        id: menuSurface

        color: "transparent"
        height: implicitHeight
        implicitHeight: menuColumn.implicitHeight + 12
        width: parent.width

        ColumnLayout {
            id: menuColumn

            // Keep the container keyboard-active when the
            // menu opens. Once an action is focused its
            // own handlers delegate to the same navigator.
            Keys.priority: Keys.BeforeItem
            spacing: 2

            Keys.onDownPressed: function (event) {
                menuNavigation.move(1);
                event.accepted = true;
            }
            Keys.onEscapePressed: function (event) {
                menuNavigation.escapeRequested();
                event.accepted = true;
            }
            Keys.onReturnPressed: function (event) {
                menuNavigation.activateCurrent();
                event.accepted = true;
            }
            Keys.onSpacePressed: function (event) {
                menuNavigation.activateCurrent();
                event.accepted = true;
            }
            Keys.onUpPressed: function (event) {
                menuNavigation.move(-1);
                event.accepted = true;
            }

            anchors {
                left: parent.left
                right: parent.right
            }
            Repeater {
                id: mediaControls

                model: appMenu.mediaPlayers

                delegate: DockMediaControls {
                    required property var modelData

                    Layout.fillWidth: true
                    accent: appIcon.accentColor
                    accentForeground: appIcon.accentForeground
                    cornerRadius: appMenu.contentRadius
                    menuActive: appMenu.visible || appMenu.closing
                    player: modelData
                }
            }
            MenuSeparator {
                visible: appMenu.mediaPlayers.length > 0
            }
            DockNotifications {
                id: notificationPreview

                Layout.fillWidth: true
                entries: button.appNotifications
                menuActive: appMenu.visible
                state: dock.notificationStore
                visible: entries.length > 0 || renderedNotificationCount > 0
            }
            MenuSeparator {
                implicitHeight: Theme.gap * notificationPreview.presence
                visible: notificationPreview.visible
            }
            MenuAction {
                cornerRadius: appMenu.contentRadius
                enabled: button.currentGroup.desktopEntry !== null || dock.isPinned(button.currentGroup)
                // Keep pinning in the same plain action group as
                // Launch new…; this action intentionally has no icon.
                iconSource: ""
                label: dock.isPinned(button.currentGroup) ? "Unpin from dock" : "Pin to dock"
                navigation: menuNavigation
                objectName: "dockPinAction"

                onTriggered: {
                    button.menuOpen = false;
                    dock.setPinned(button.currentGroup, !dock.isPinned(button.currentGroup));
                }
            }
            MenuAction {
                cornerRadius: appMenu.contentRadius
                label: "Launch new..."
                navigation: menuNavigation
                visible: button.currentGroup.desktopEntry !== null

                onTriggered: {
                    dock.launch(button.currentGroup, true);
                    button.menuOpen = false;
                }
            }
            MenuSeparator {
                visible: desktopActions.count > 0
            }
            MenuSection {
                label: button.currentGroup.desktopEntry?.name || button.currentGroup.id
                visible: desktopActions.count > 0
            }
            Repeater {
                id: desktopActions

                model: button.currentGroup.windows.length > 0 && button.currentGroup.desktopEntry ? button.currentGroup.desktopEntry.actions : []

                delegate: MenuAction {
                    required property var modelData

                    cornerRadius: appMenu.contentRadius
                    label: dock.menuLabel(modelData.name, "Application action")
                    navigation: menuNavigation

                    onTriggered: {
                        const helper = Quickshell.env("BINGUX_APP_LAUNCHER_HELPER");
                        const command = helper ? [helper] : ["python3", decodeURIComponent(Qt.resolvedUrl("launch-application.py").toString().replace(/^file:\/\//, ""))];
                        Quickshell.execDetached(command.concat(["--notify-errors", "--action=" + modelData.id, "--", button.currentGroup.desktopEntry.id]));
                        button.menuOpen = false;
                    }
                }
            }
            MenuSeparator {
                visible: button.currentGroup.windows.length > 0 || !!dock.launchAttempts[button.currentGroup.id]
            }
            MenuSection {
                label: "Open windows"
                visible: button.currentGroup.windows.length > 0 || !!dock.launchAttempts[button.currentGroup.id]
            }
            Repeater {
                delegate: MenuAction {
                    readonly property bool isLoading: !!modelData?.loading
                    required property var modelData

                    closable: isLoading || !!modelData
                    cornerRadius: appMenu.contentRadius
                    iconSource: appIcon.source
                    label: isLoading ? "Loading..." : dock.menuLabel(modelData && modelData.title, button.currentGroup.id)
                    loading: isLoading
                    navigation: menuNavigation
                    selectedWindow: !isLoading && !!modelData && (modelData.activated || (appMenu.visible && !ToplevelManager.activeToplevel && dock.lastActiveWindow === modelData))

                    onCloseRequested: {
                        if (isLoading) {
                            dock.cancelLaunch(button.currentGroup.id);
                        } else if (modelData) {
                            appMenu.dismissOnOutsideClick = false;
                            closeClickGuard.restart();
                            const window = modelData;
                            Qt.callLater(() => {
                                if (window)
                                    window.close();
                            });
                        }
                    }
                    onTriggered: {
                        if (!isLoading && modelData)
                            modelData.activate();
                        if (!isLoading)
                            button.menuOpen = false;
                    }
                }
                model: ScriptModel {
                    values: button.currentGroup.windows.concat(dock.launchAttempts[button.currentGroup.id] ? [
                        {
                            loading: true
                        }
                    ] : [])
                }
            }
        }
    }

    component MenuAction: ActionButton {
        id: action

        property bool closable: false
        property bool closing: false
        property url iconSource: ""
        required property string label
        property bool loading: false
        readonly property bool menuEntry: true
        property var navigation: null
        property bool selectedWindow: false

        signal closeRequested
        signal triggered

        Keys.priority: Keys.BeforeItem
        Layout.fillWidth: true
        flat: true
        implicitHeight: 38
        leftPadding: Theme.padding
        rightPadding: closable ? Theme.spaceSmall : Theme.padding
        showFocusRing: navigation !== null && navigation.keyboardNavigation && activeFocus
        text: label

        contentItem: Item {
            RowLayout {
                anchors.fill: parent
                spacing: Theme.gap

                OsIconImage {
                    Layout.alignment: Qt.AlignVCenter
                    implicitSize: Theme.iconSize
                    source: action.iconSource
                    visible: !action.loading && action.iconSource.toString().length > 0
                }
                PreviewSpinner {
                    Layout.alignment: Qt.AlignVCenter
                    colour: Theme.accent
                    implicitHeight: Theme.iconSize
                    implicitWidth: Theme.iconSize
                    loading: action.loading
                    visible: action.loading
                }
                Text {
                    Layout.fillWidth: true
                    color: action.selectedWindow ? Theme.accent : action.loading ? Theme.muted : Theme.text
                    elide: Text.ElideRight
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    font.weight: action.selectedWindow ? Font.Medium : Font.Normal
                    text: action.label
                    textFormat: Text.PlainText
                    verticalAlignment: Text.AlignVCenter
                }
                ActionButton {
                    id: closeWindow

                    Accessible.name: action.loading ? "Cancel loading" : (action.closing ? "Closing " : "Close ") + action.label
                    Layout.alignment: Qt.AlignVCenter
                    Layout.preferredHeight: 28
                    Layout.preferredWidth: 28
                    enabled: !action.closing
                    flat: true
                    horizontalPadding: Theme.spaceSmall
                    hoverEnabled: true
                    objectName: "closeWindowButton"
                    showFocusRing: action.navigation !== null && action.navigation.keyboardNavigation
                    visible: action.closable

                    background: Rectangle {
                        border.color: Theme.accent
                        border.width: closeWindow.showFocusRing && closeWindow.activeFocus ? 2 : 0
                        // The menu row is already hovered underneath this
                        // button, so use a brighter surface for its own state.
                        color: closeWindow.down ? Qt.lighter(Theme.pressed, 1.2) : closeWindow.hovered ? Theme.pressed : "transparent"
                        radius: closeWindow.cornerRadius
                    }
                    contentItem: Item {
                        Text {
                            anchors.centerIn: parent
                            color: Theme.text
                            font.pixelSize: 22
                            text: "\u00d7"
                            visible: !action.closing
                        }
                        PreviewSpinner {
                            Accessible.name: "Closing " + action.label
                            anchors.centerIn: parent
                            colour: Theme.text
                            implicitHeight: 22
                            implicitWidth: 22
                            loading: action.closing
                        }
                    }

                    onClicked: {
                        if (action.navigation)
                            action.navigation.pointerActivate();
                        action.closing = true;
                        closeRetry.restart();
                        action.closeRequested();
                    }
                }
            }
        }

        Keys.onDownPressed: function (event) {
            if (navigation) {
                navigation.move(1);
                event.accepted = true;
            }
        }
        Keys.onEscapePressed: function (event) {
            if (navigation) {
                navigation.escapeRequested();
                event.accepted = true;
            }
        }
        Keys.onUpPressed: function (event) {
            if (navigation) {
                navigation.move(-1);
                event.accepted = true;
            }
        }
        onClicked: {
            if (navigation)
                navigation.pointerActivate();
            triggered();
        }

        Timer {
            id: closeRetry

            interval: 10000

            onTriggered: action.closing = false
        }
    }
    component MenuSection: Item {
        required property string label

        Layout.fillWidth: true
        implicitHeight: sectionLabel.implicitHeight + Theme.spaceSmall

        Text {
            id: sectionLabel

            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            color: Theme.muted
            elide: Text.ElideRight
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            leftPadding: Theme.padding
            rightPadding: Theme.padding
            text: parent.label
        }
    }
    component MenuSeparator: Item {
        Layout.fillWidth: true
        implicitHeight: Theme.gap

        Rectangle {
            anchors.left: parent.left
            anchors.leftMargin: Theme.padding
            anchors.right: parent.right
            anchors.rightMargin: Theme.padding
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.outline
            height: 1
            opacity: 0.8
        }
    }
}
