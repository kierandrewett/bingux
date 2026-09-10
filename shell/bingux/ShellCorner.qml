import QtQuick
import QtQuick.Shapes

// Fill the outside of a rounded desktop corner with the shell surface.
Item {
    id: root
    SurfaceFade { target: root }
    property color color: Theme.barBackground
    property color borderColor: Theme.barDivider
    property bool mirrored: false
    implicitWidth: Theme.shellRadius
    implicitHeight: implicitWidth
    Shape {
        anchors.fill: parent
        antialiasing: true
        preferredRendererType: Shape.CurveRenderer
        transform: Scale { origin.x: root.width / 2; xScale: root.mirrored ? -1 : 1 }
        ShapePath {
            fillColor: root.color
            strokeWidth: -1
            startX: 0; startY: 0
            PathLine { x: root.width; y: 0 }
            // Keep an opaque surface beneath the translucent grey stroke.
            PathLine { x: root.width; y: 1 }
            PathArc {
                x: 1; y: root.height
                radiusX: Math.max(0.5, root.width - 1)
                radiusY: Math.max(0.5, root.height - 1)
                direction: PathArc.Counterclockwise
            }
            PathLine { x: 0; y: root.height }
            PathLine { x: 0; y: 0 }
        }
        ShapePath {
            fillColor: "transparent"
            strokeColor: Theme.panelOuterOutline
            strokeWidth: 1
            capStyle: ShapePath.FlatCap
            startX: root.width; startY: 1.5
            PathArc {
                x: 1.5; y: root.height
                radiusX: Math.max(0.5, root.width - 1.5)
                radiusY: Math.max(0.5, root.height - 1.5)
                direction: PathArc.Counterclockwise
            }
        }
        ShapePath {
            fillColor: "transparent"
            strokeColor: root.borderColor
            strokeWidth: 1
            capStyle: ShapePath.FlatCap
            startX: root.width; startY: 0.5
            PathArc {
                x: 0.5; y: root.height
                radiusX: Math.max(0.5, root.width - 0.5)
                radiusY: Math.max(0.5, root.height - 0.5)
                direction: PathArc.Counterclockwise
            }
        }
    }
}
