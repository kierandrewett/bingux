import QtQuick

Item {
    id: root
    property int count: 0
    visible: count > 0
    implicitWidth: badge.implicitWidth
    implicitHeight: 20
    Accessible.role: Accessible.StaticText
    Accessible.name: count + (count === 1 ? " notification" : " notifications")
    function playArchive() {
        if (!Theme.reducedMotion && count > 0) archiveLaunch.restart();
    }
    Rectangle {
        id: echo
        anchors.fill: parent
        radius: height / 2
        color: Theme.accent
        opacity: 0
        visible: archiveLaunch.running
        Text {
            anchors.centerIn: parent
            text: root.count
            color: Theme.shellSurface
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
    ParallelAnimation {
        id: archiveLaunch
        NumberAnimation { target: echo; property: "scale"; from: 1; to: 2.4; duration: 300; easing.type: Easing.OutCubic }
        NumberAnimation { target: echo; property: "opacity"; from: 0.75; to: 0; duration: 300; easing.type: Easing.OutCubic }
        SequentialAnimation {
            NumberAnimation { target: badge; property: "scale"; from: 1; to: 1.15; duration: 100; easing.type: Easing.OutCubic }
            NumberAnimation { target: badge; property: "scale"; to: 1; duration: 200; easing.type: Easing.OutCubic }
        }
    }
    Rectangle {
        id: badge
        objectName: "notificationCountBadge"
        anchors.fill: parent
        visible: root.count > 0
        implicitWidth: Math.max(20, number.implicitWidth + Theme.gap)
        implicitHeight: 20
        radius: height / 2
        color: Theme.accent
        AnimatedCount {
            id: number
            objectName: "notificationIndicatorCount"
            anchors.centerIn: parent
            value: Math.max(0, root.count)
            color: Theme.shellSurface
        }
    }
}
