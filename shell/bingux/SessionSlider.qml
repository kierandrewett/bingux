import QtQuick

// Minimal horizontal slider on the lock screen's glass: a translucent track,
// a white fill and a round handle. Dragging or clicking emits moved(value).
Item {
    id: root

    property real value: 0
    signal moved(real value)

    implicitHeight: 20
    implicitWidth: 120

    function valueAt(x) {
        return Math.max(0, Math.min(1, (x - handle.width / 2) / (width - handle.width)));
    }

    Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        height: 4
        radius: 2
        color: Qt.rgba(1, 1, 1, 0.22)
        Rectangle {
            width: Math.round(handle.x + handle.width / 2)
            height: parent.height
            radius: 2
            color: "#ffffff"
        }
    }
    Rectangle {
        id: handle
        width: 16
        height: 16
        radius: 8
        anchors.verticalCenter: parent.verticalCenter
        x: Math.round(root.value * (root.width - width))
        color: "#ffffff"
        border.width: 1
        border.color: Qt.rgba(0, 0, 0, 0.12)
    }
    MouseArea {
        anchors.fill: parent
        onPressed: mouse => root.moved(root.valueAt(mouse.x))
        onPositionChanged: mouse => {
            if (pressed)
                root.moved(root.valueAt(mouse.x));
        }
    }
}
