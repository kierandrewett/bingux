import QtQuick
import Quickshell

Rectangle {
    id: root
    property bool shown: false
    readonly property int cutoutMargin: 2
    property int count: 0
    property string iconName: ""
    property color foreground: Theme.shellSurface
    property int fadeOutDuration: Theme.motion
    readonly property string label: count > 99 ? "99+" : String(count)
    height: Theme.dockBadgeSize
    width: iconName ? height : Math.max(height, number.implicitWidth + Theme.padding)
    radius: height / 2
    opacity: 0
    onShownChanged: {
        fade.stop();
        fade.to = shown ? 1 : 0;
        fade.duration = Theme.reducedMotion ? 0 : shown ? Theme.motion : fadeOutDuration;
        fade.start();
    }
    Component.onCompleted: opacity = shown ? 1 : 0
    visible: opacity > 0
    NumberAnimation {
        id: fade
        target: root
        property: "opacity"
    }
    Behavior on width {
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
        }
    }
    AnimatedCount {
        id: number
        objectName: "dockBadgeCount"
        anchors.centerIn: parent
        visible: !root.iconName
        value: Math.max(1, root.count)
        color: root.foreground
        font.pixelSize: 13
        font.weight: Font.Bold
        opticalCenter: true
    }
    SymbolicIcon {
        anchors.centerIn: parent
        visible: !!root.iconName
        source: root.iconName ? Quickshell.iconPath(root.iconName) : ""
        implicitSize: Theme.fontSmall
        color: root.foreground
    }
}
