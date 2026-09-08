import QtQuick
import QtQuick.Shapes

Item {
    id: root
    property bool playing: false
    property color color: Theme.text
    property real progress: playing ? 0 : 1
    implicitWidth: Theme.iconSize
    implicitHeight: Theme.iconSize
    Behavior on progress { NumberAnimation { duration: Theme.reducedMotion ? 0 : Theme.mediaActionMotion; easing.type: Easing.InOutCubic } }
    function blend(pause, play) { return pause + (play - pause) * progress; }
    Shape {
        width: 20; height: 20
        scale: root.width / 20
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeWidth: -1
            fillColor: root.color
            startX: root.blend(4, 5); startY: 3
            PathLine { x: root.blend(8, 10); y: root.blend(3, 5.9167) }
            PathLine { x: root.blend(8, 10); y: root.blend(17, 14.0833) }
            PathLine { x: root.blend(4, 5); y: 17 }
            PathLine { x: root.blend(4, 5); y: 3 }
            PathMove { x: root.blend(12, 10); y: root.blend(3, 5.9167) }
            PathLine { x: root.blend(16, 17); y: root.blend(3, 10) }
            PathLine { x: root.blend(16, 17); y: root.blend(17, 10) }
            PathLine { x: root.blend(12, 10); y: root.blend(17, 14.0833) }
            PathLine { x: root.blend(12, 10); y: root.blend(3, 5.9167) }
        }
    }
}
