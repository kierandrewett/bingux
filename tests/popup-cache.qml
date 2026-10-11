import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Real-seat fixture: drive fixture IPC and mouse/keyboard input in a VM.
ShellRoot {
    id: root
    property int backgroundPresses: 0
    property int cardPresses: 0
    property int childPresses: 0
    property var firstCardWindow: null
    property var firstDismissWindow: null
    property int motionDuration: Theme.popupOpenMotion
    PanelWindow {
        id: background
        screen: popup.screen
        color: "#254535"
        WlrLayershell.layer: WlrLayer.Bottom
        WlrLayershell.namespace: "bingux-cache-test-background"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        exclusionMode: ExclusionMode.Ignore
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        Repeater {
            model: Math.ceil(background.width / 32) * Math.ceil(background.height / 32)
            Rectangle {
                required property int index
                readonly property int columns: Math.ceil(background.width / 32)
                x: index % columns * 32
                y: Math.floor(index / columns) * 32
                width: 32
                height: 32
                color: (index % columns + Math.floor(index / columns)) % 2 ? "#284636" : "#839a75"
            }
        }
        MouseArea {
            anchors.fill: parent
            onPressed: root.backgroundPresses++
        }
    }
    ShellPopup {
        id: popup
        preferredX: 180
        preferredY: 160
        popupWidth: 280
        popupHeight: 180
        surfaceColor: Qt.rgba(0.15, 0.20, 0.17, 0.82)
        openMotion: root.motionDuration
        closeMotion: root.motionDuration
        windowShadow: true
        Rectangle {
            id: button
            width: 200
            height: 90
            color: "#496955"
            MouseArea {
                anchors.fill: parent
                onPressed: root.cardPresses++
            }
        }
    }
    ShellPopup {
        id: other
        screen: popup.screen
        preferredX: 640
        preferredY: 160
        popupWidth: 280
        popupHeight: 180
    }
    ShellPopup {
        id: child
        hostItem: popup.contentItem
        anchorItem: button
        anchorWindow: popup.nativeWindow
        preferredX: 200
        preferredY: 240
        popupWidth: 180
        popupHeight: 100
        onAboutToOpen: Qt.callLater(childNavigation.focusMenu)
        MenuNavigator {
            id: childNavigation
            onEscapeRequested: child.visible = false
        }
        MouseArea {
            anchors.fill: parent
            onPressed: root.childPresses++
        }
    }
    QtObject {
        id: notification
        property int id: 100
        property real expireTimeout: 0
        property string appName: "Files"
        property string desktopEntry: ""
        property string appIcon: "system-file-manager"
        property string summary: "Popup cache check"
        property string body: "History and toasts remain usable after closing."
        property var actions: []
        property bool tracked: false
        signal closed(int reason)
        function dismiss() {
        }
        function expire() {
        }
    }
    NotificationState {
        id: notifications
    }
    NotificationSurface {
        id: notificationsWindow
        screen: popup.screen
        state: notifications
        notificationCentre: history
    }
    NotificationHistoryPopup {
        id: history
        screen: popup.screen
        notificationSurface: notificationsWindow
    }
    IpcHandler {
        target: "fixture"
        function open(which: string): void {
            if (which === "child")
                child.visible = true;
            else if (which === "other")
                other.visible = true;
            else if (which === "history")
                history.visible = true;
            else
                popup.visible = true;
        }
        function close(): void {
            child.visible = false;
            popup.visible = false;
            other.visible = false;
            history.visible = false;
        }
        function slow(): void {
            root.motionDuration = 1200;
        }
        function notify(): void {
            notification.id++;
            notifications.accept(notification);
        }
        function clear(): void {
            notifications.dismissAll();
        }
        function remember(): void {
            firstCardWindow = popup.contentItem.Window.window;
            firstDismissWindow = popup.testDismissContentItem.Window.window;
        }
        function screen(index: int): void {
            popup.screen = Quickshell.screens[index];
        }
        function status(): string {
            return JSON.stringify({
                open: popup.visible,
                child: child.visible,
                other: other.visible,
                opacity: popup.presentationOpacity,
                materialAlpha: popup.surfaceColor.a,
                retained: popup.retained,
                drawn: popup.body.parent.visible,
                inputWidth: popup.nativeWindow.mask.width,
                inputHeight: popup.nativeWindow.mask.height,
                dismissWidth: popup.testDismissWindow.mask.width,
                keyboard: popup.nativeWindow.WlrLayershell.keyboardFocus,
                sameCard: firstCardWindow === popup.contentItem.Window.window,
                sameDismiss: firstDismissWindow === popup.testDismissContentItem.Window.window,
                backgroundPresses: root.backgroundPresses,
                cardPresses: root.cardPresses,
                childPresses: root.childPresses,
                history: history.visible,
                historyRetained: history.retained,
                notifications: notificationsWindow.notificationCount,
                renderedNotifications: notificationsWindow.renderedNotificationCount,
                notificationPresented: notificationsWindow.presented,
                notificationDrawn: notificationsWindow.desktopViewport.visible,
                notificationInput: notificationsWindow.mask.height,
                notificationKeyboard: notificationsWindow.WlrLayershell.keyboardFocus,
                notificationFocus: notificationsWindow.contentItem.activeFocus,
                historyFocus: notificationsWindow.desktopViewport.activeFocus,
                screen: popup.screen.name
            });
        }
        function quit(): void {
            Qt.quit();
        }
    }
}
