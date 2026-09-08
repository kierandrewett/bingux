import QtQuick

QtObject {
    readonly property bool dockEnabled: BinguxPreferences.data.desktop.dock
    readonly property bool sidebarEnabled: BinguxPreferences.data.desktop.sidebar
    readonly property bool metricsEnabled: BinguxPreferences.data.desktop.metrics
    readonly property string gnoblinCtlPath: "gnoblinctl"
    readonly property string notificationDbusPath: "gdbus"
    readonly property string timeoutPath: "timeout"
    readonly property var pinnedApps: []
}
