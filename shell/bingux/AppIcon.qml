import QtQuick
import Quickshell
import "MediaMatch.js" as MediaMatch

// One app image and activity presentation for the dock and window switcher.
Item {
    id: root
    property var group: null
    property var presentation: null
    property var activeStreams: []
    property var notifications: []
    property int implicitSize: Theme.dockIconSize
    property alias source: image.source
    readonly property string normalizedSource: image.normalizedSource
    readonly property color accentColor: accent.color
    readonly property color accentForeground: accent.foreground
    readonly property bool playingAudio: activeStreams.some(node => MediaMatch.matchesAudio(node.properties, group))
    readonly property var appNotifications: notifications.filter(entry => MediaMatch.matchesNotification(entry, group))
    readonly property int notificationCount: appNotifications.length
    readonly property string appName: presentation?.label || (group && group.desktopEntry ? group.desktopEntry.name : group ? group.displayName || group.id : "")
    readonly property string tooltipText: appName + (playingAudio ? " · Playing audio" : "")
        + (notificationCount > 0 ? " · " + notificationCount + " notifications" : "")
    implicitWidth: implicitSize
    implicitHeight: implicitSize

    OsIconImage {
        id: image
        visible: root.presentation?.showIcon ?? true
        anchors.fill: parent
        anchors.bottomMargin: root.presentation?.showText && root.presentation?.showIcon ? 16 : 0
        implicitSize: root.implicitSize
        source: Quickshell.iconPath(root.presentation?.icon || (root.group && root.group.desktopEntry && root.group.desktopEntry.icon
            ? root.group.desktopEntry.icon : "application-x-executable"), "application-x-executable")
    }
    Text {
        visible: !!root.presentation?.showText
        anchors.bottom: parent.bottom
        width: parent.width; height: root.presentation?.showIcon ? 16 : parent.height
        text: root.appName; textFormat: Text.PlainText; elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        wrapMode: root.presentation?.showIcon ? Text.NoWrap : Text.WordWrap
        color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
    }
    IconAccent { id: accent; source: image.normalizedSource }
    DockBadge {
        objectName: "dockAudioBadge"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: -Theme.spaceSmall
        fadeOutDuration: Theme.audioBadgeFadeOut
        shown: root.playingAudio
        iconName: "audio-volume-high-symbolic"
        color: root.accentColor
        foreground: root.accentForeground
    }
    DockBadge {
        objectName: "dockNotificationBadge"
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: -Theme.spaceSmall
        shown: root.notificationCount > 0
        count: root.notificationCount
        color: Theme.notificationBadge
        foreground: Theme.text
    }
}
