import QtQuick
import QtQuick.Layouts

NotificationStack {
    id: root
    objectName: "dockNotifications"
    property var entries: []
    property bool menuActive: false
    presentedEntries: entries
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
