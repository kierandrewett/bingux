import QtQuick

QtObject {
    readonly property bool dockEnabled: true
    readonly property bool metricsEnabled: true
    readonly property string gnoblinCtlPath: "gnoblinctl"
    readonly property string timeoutPath: "timeout"
    readonly property var pinnedApps: []
}
