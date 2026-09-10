import QtQuick
import QtQuick.Effects
import Quickshell
import "MediaMatch.js" as MediaMatch

// One app image and activity presentation for the dock and window switcher.
Item {
    id: root
    property var group: null
    property var presentation: null
    property var activeStreams: []
    property var notifications: []
    property var notificationIndex: MediaMatch.notificationIndex(notifications)
    property var additionalBadges: []
    readonly property var badges: [audioBadge, notificationBadge].concat(additionalBadges)
    readonly property bool hasBadges: badges.some(badge => badge.visible && badge.opacity > 0)
    property int implicitSize: Theme.dockIconSize
    property alias source: image.source
    readonly property string normalizedSource: image.normalizedSource
    readonly property color accentColor: accent.color
    readonly property color accentForeground: accent.foreground
    readonly property bool playingAudio: activeStreams.some(node => MediaMatch.matchesAudio(node.properties, group))
    readonly property var appNotifications: MediaMatch.notificationsForGroup(notificationIndex, group)
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
        layer.enabled: root.hasBadges
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: badgeMaskTexture
            maskInverted: true
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }
        source: Quickshell.iconPath(root.presentation?.icon || (root.group && root.group.desktopEntry && root.group.desktopEntry.icon
            ? root.group.desktopEntry.icon : "application-x-executable"), "application-x-executable")
    }
    // The mask removes icon pixels, so hover, selection and wallpaper show
    // through the clearance around each badge. Track the animated badge shape.
    Item {
        id: badgeMask
        width: image.width
        height: image.height
        visible: root.hasBadges
        Repeater {
            model: root.badges
            Rectangle {
                required property var modelData
                readonly property point position: image.mapFromItem(modelData.parent, modelData.x, modelData.y)
                x: position.x - modelData.cutoutMargin
                y: position.y - modelData.cutoutMargin
                width: modelData.width + modelData.cutoutMargin * 2
                height: modelData.height + modelData.cutoutMargin * 2
                radius: modelData.radius + modelData.cutoutMargin
                color: "white"
                opacity: modelData.visible ? modelData.opacity : 0
                antialiasing: true
            }
        }
    }
    ShaderEffectSource {
        id: badgeMaskTexture
        sourceItem: badgeMask
        hideSource: true
        live: root.hasBadges
        visible: false
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
        id: audioBadge
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
        id: notificationBadge
        objectName: "dockNotificationBadge"
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: -Theme.spaceSmall
        shown: root.notificationCount > 0
        count: root.notificationCount
        color: Theme.notificationBadge
        foreground: "#ffffff"
    }
}
