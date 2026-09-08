import QtQuick
import QtQuick.Layouts

NotificationStack {
    id: root
    objectName: "dockNotifications"
    property var entries: []
    property bool menuActive: false
    // Hidden app menus must not rebuild notification cards on every model update.
    presentedEntries: menuActive ? entries : []
    onMenuActiveChanged: if (!menuActive) Qt.callLater(root.resetPresentation)
    historyMode: true
    groupNotifications: false
    animationsEnabled: false
    collapseOnDismiss: true
    swipeEnabled: false
    cardBackground: Theme.menuWidgetBackground
    cardRadius: Theme.menuWidgetRadius
    cardBorder: false
    cardShadow: false
    active: menuActive
    trailingInset: 0
    bottomInset: 0
    Layout.preferredHeight: Math.min(Math.ceil(stackHeight), 320)
}
