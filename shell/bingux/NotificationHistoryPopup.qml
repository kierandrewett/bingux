import QtQuick
import QtQuick.Controls

// Independent notification history, opened from the bell in the top bar.
ShellPopup {
    id: root
    required property var notificationSurface
    hostItem: notificationSurface ? notificationSurface.desktopViewport : null
    readonly property bool hasNotifications: notificationSurface && notificationSurface.renderedNotificationCount > 0
    // The list and footer slide together, clipped by the desktop viewport.
    initialRevealScale: 1
    closeMotion: Theme.reducedMotion ? 0 : 300
    closeEasing: Easing.InCubic
    property real slideOffset: 0
    onAboutToOpen: {
        if (notificationSurface)
            notificationSurface.prepareForHistory();
        slide.stop();
        if (!retained)
            slideOffset = Theme.reducedMotion ? 0 : width - panelX;
        slide.to = 0;
        slide.start();
    }
    onClosingChanged: if (closing) {
        slide.stop();
        slide.to = width - panelX;
        slide.start();
    }
    NumberAnimation {
        id: slide
        target: root
        property: "slideOffset"
        to: 0
        duration: Theme.reducedMotion ? 0 : 300
        easing.type: Easing.InOutCubic
    }
    Connections {
        target: root.notificationSurface
        function onRenderedNotificationCountChanged() {
            if (root.notificationSurface.renderedNotificationCount === 0 && root.notificationSurface.notificationCount === 0)
                root.visible = false;
        }
    }
    popupWidth: Theme.notificationWidth
    contentPadding: 0
    surfaceVisible: false
    property real dockSafeInset: Theme.dockExclusiveHeight
    preferredX: width - popupWidth - Theme.padding
    preferredY: Theme.barHeight + Theme.gap
    readonly property real listHeight: Math.min(notificationSurface ? notificationSurface.stackHeight : 0, Math.max(0, Math.min(height * 0.8 - 40, height - dockSafeInset - Theme.gap - preferredY - 40)))
    readonly property real listX: panelX + slideOffset
    readonly property real listY: panelY
    popupHeight: listHeight + 40

    ActionButton {
        objectName: "controlClearNotifications"
        x: root.slideOffset + parent.width - width
        y: root.listHeight + 4
        implicitHeight: 28
        cornerRadius: 10
        text: "Clear all"
        Accessible.name: "Clear all notifications"
        onClicked: root.notificationSurface.dismissAll()
    }
}
