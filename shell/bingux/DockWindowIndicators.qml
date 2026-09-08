import QtQuick
import Quickshell

Item {
    id: root
    property var windows: []
    readonly property int windowCount: windows.length
    readonly property int visibleCount: Math.min(4, windowCount)
    readonly property int activeIndex: {
        for (let i = 0; i < windows.length; i++) {
            if (windows[i]?.activated)
                return i;
        }
        return -1;
    }
    property int firstVisibleIndex: 0
    readonly property bool moreBefore: firstVisibleIndex > 0
    readonly property bool moreAfter: firstVisibleIndex + visibleCount < windowCount
    readonly property real scrollOffset: -strip.contentX
    implicitWidth: visibleCount > 0 ? visibleCount * 8 + 10 + (activeIndex >= firstVisibleIndex && activeIndex < firstVisibleIndex + visibleCount ? 10 : 0) : 0
    implicitHeight: 8
    visible: width > 0
    Behavior on implicitWidth { NumberAnimation { duration: Theme.reducedMotion ? 0 : 220; easing.type: Easing.OutCubic } }
    Accessible.role: Accessible.StaticText
    Accessible.name: windowCount + " windows" + (activeIndex >= 0 ? ", window " + (activeIndex + 1) + " active" : "")

    function revealActiveWindow() {
        let start = Math.min(firstVisibleIndex, Math.max(0, windowCount - 4));
        if (activeIndex >= 0) {
            if (activeIndex < start)
                start = activeIndex;
            else if (activeIndex >= start + 4)
                start = activeIndex - 3;
        }
        firstVisibleIndex = start;
    }
    onActiveIndexChanged: revealActiveWindow()
    onWindowCountChanged: revealActiveWindow()
    Component.onCompleted: revealActiveWindow()

    ListView {
        id: strip
        x: 5
        width: Math.max(0, root.width - 10)
        height: parent.height
        clip: true
        orientation: ListView.Horizontal
        interactive: false
        cacheBuffer: Math.max(32, root.windowCount * 8)
        boundsBehavior: Flickable.StopAtBounds
        contentX: root.firstVisibleIndex * 8 + (root.activeIndex >= 0 && root.activeIndex < root.firstVisibleIndex ? 10 : 0)
        Behavior on contentX {
            NumberAnimation { duration: Theme.reducedMotion ? 0 : 240; easing.type: Easing.OutQuart }
        }
        model: ScriptModel { values: root.windows }
        displaced: Transition {
            NumberAnimation { property: "x"; duration: Theme.reducedMotion ? 0 : 220; easing.type: Easing.OutCubic }
        }
        delegate: Item {
            id: dot
            opacity: 0
            scale: 0.5
            Component.onCompleted: appear.start()
            ListView.onRemove: {
                ListView.delayRemove = true;
                appear.stop();
                disappear.start();
            }
            ParallelAnimation {
                id: appear
                NumberAnimation { target: dot; property: "opacity"; to: 1; duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutQuad }
                NumberAnimation { target: dot; property: "scale"; to: 1; duration: Theme.reducedMotion ? 0 : 240; easing.type: Easing.OutBack; easing.overshoot: 0.6 }
            }
            SequentialAnimation {
                id: disappear
                ParallelAnimation {
                    NumberAnimation { target: dot; property: "opacity"; to: 0; duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.InQuad }
                    NumberAnimation { target: dot; property: "scale"; to: 0.5; duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.InCubic }
                }
                PropertyAction { target: dot; property: "ListView.delayRemove"; value: false }
            }
            required property var modelData
            width: modelData?.activated ? 18 : 8
            Behavior on width { NumberAnimation { duration: Theme.reducedMotion ? 0 : 220; easing.type: Easing.OutCubic } }
            height: 8
            Rectangle {
                anchors.centerIn: parent
                width: modelData?.activated ? 16 : 6
                height: 6
                radius: 3
                color: modelData?.activated ? Theme.accent : Theme.muted
                Behavior on width { NumberAnimation { duration: Theme.reducedMotion ? 0 : 220; easing.type: Easing.OutCubic } }
                Behavior on color { ColorAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.InOutSine } }
            }
        }
    }
    Rectangle {
        x: 0; y: 3; width: 2; height: 2; radius: 1
        color: Theme.muted
        opacity: root.moreBefore ? 0.5 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.InOutSine } }
    }
    Rectangle {
        x: parent.width - width; y: 3; width: 2; height: 2; radius: 1
        color: Theme.muted
        opacity: root.moreAfter ? 0.5 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.InOutSine } }
    }
}
