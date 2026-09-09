import QtQuick
import QtQuick.Layouts

NotificationStack {
    id: root
    objectName: "dockNotifications"
    property var entries: []
    property bool menuActive: false
    // Keep a prepared snapshot while closed. Incoming notifications must not
    // rebuild hidden cards; refresh them only on hover or an open request.
    property var preparedEntries: []
    function prepare() {
        if (preparedEntries.length !== entries.length || entries.some((entry, index) => entry !== preparedEntries[index]))
            preparedEntries = entries.slice();
    }
    presentedEntries: menuActive ? entries : preparedEntries
    onMenuActiveChanged: {
        if (menuActive) prepare();
        else preparedEntries = entries.slice();
    }
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
