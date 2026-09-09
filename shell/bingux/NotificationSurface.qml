import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    required property var state
    property var notificationCentre: null
    property var sidebarScreen: null
    property real leftInset: 0
    property real rightInset: 0
    readonly property bool hasSidebar: sidebarScreen !== null && screen !== null && sidebarScreen.name === screen.name
    readonly property alias desktopViewport: desktopArea
    property bool inputSuspended: false
    readonly property bool inHistory: notificationCentre !== null && notificationCentre.retained
    readonly property real stackHeight: stack.stackHeight
    readonly property int notificationCount: state.allEntries.length
    readonly property int renderedNotificationCount: stack.renderedNotificationCount
    readonly property alias viewport: stack
    function prepareForHistory() { if (!inHistory) stack.prepareHistory(); }
    function dismissAll() { state.dismissAll(); }
    function toggleGroup(key) { stack.toggleGroup(key); }
    onInHistoryChanged: if (!inHistory) {
        state.archiveToasts();
        stack.resetPresentation();
    }
    color: "transparent"
    focusable: inHistory && notificationCentre.visible
    // Retain history in QML, but unmap its full-screen buffer when no cards
    // are shown. An invisible mapped surface still pays compositor blur costs.
    visible: !inputSuspended && (renderedNotificationCount > 0 || inHistory)
    exclusionMode: ExclusionMode.Ignore
    surfaceFormat.opaque: false
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-notifications"
    BlurRegion {
        window: root
        surfaceNamespace: "bingux-notifications"
        // Include the footer and shadow margin. Sliding cards remain inside
        // this final-position rectangle until the desktop clips them away.
        region: root.inHistory
            ? Qt.rect(desktopArea.x + root.notificationCentre.panelX - Theme.padding,
                Math.max(0, root.notificationCentre.panelY - Theme.padding),
                root.notificationCentre.popupWidth + Theme.padding * 2,
                root.notificationCentre.popupHeight + Theme.padding * 2)
            : Qt.rect(desktopArea.x + stack.x - Theme.padding, Math.max(0, stack.y - Theme.padding),
                stack.width + Theme.padding * 2, stack.height + Theme.padding * 2)
    }
    WlrLayershell.keyboardFocus: inHistory && notificationCentre.visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    anchors { top: true; bottom: true; left: true; right: true }

    // Both toasts and history live in the desktop area. Clipping here also
    // contains the whole centre's entrance at the sidebar edge.
    Item {
        id: desktopArea
        objectName: "notificationDesktopArea"
        x: root.hasSidebar ? root.leftInset : 0
        // Before the first map the native window has no configured size yet.
        // Prepare the slide from screen geometry, not a zero-width viewport.
        width: Math.max(0, (root.width || root.screen?.width || 0) - x - (root.hasSidebar ? root.rightInset : 0))
        height: root.height || root.screen?.height || 0
        clip: true

        NotificationStack {
            id: stack
            state: root.state
            onNotificationActivated: if (root.notificationCentre) root.notificationCentre.visible = false;
            presentedEntries: root.state.allEntries
            filterToasts: true
            historyMode: root.inHistory
            groupNotifications: root.inHistory
            z: 10
            presentationOpacity: root.inHistory ? root.notificationCentre.body.parent.opacity : 1
            enabled: !root.inHistory || root.notificationCentre.visible
            width: Math.max(0, Math.min(Theme.notificationWidth + Theme.padding, parent.width - Theme.padding))
            height: Math.min(contentHeight, root.inHistory ? root.notificationCentre.listHeight
                : Math.max(0, parent.height - Theme.barHeight - Theme.gap - Theme.padding))
            x: root.inHistory ? root.notificationCentre.listX : parent.width - width
            y: root.inHistory ? root.notificationCentre.listY : Theme.barHeight + Theme.gap
        }
    }
    contentItem.Keys.onEscapePressed: if (notificationCentre) notificationCentre.visible = false;
    mask: Region {
        x: desktopArea.x + (root.inHistory ? 0 : stack.x)
        y: root.inHistory ? 0 : stack.y
        width: root.inHistory ? desktopArea.width : stack.width
        height: root.inHistory ? root.height : root.renderedNotificationCount > 0 ? stack.height : 0
    }
}
