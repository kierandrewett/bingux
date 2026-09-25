import QtQuick
import Quickshell
import Quickshell.Wayland

Scope {
    id: root
    required property bool active
    required property rect region

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: shade
            required property var modelData
            screen: modelData
            visible: root.active
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            // Both selection and recording guides are screen-aligned capture
            // geometry. Reuse the no-blur/no-opacity/no-motion surface policy.
            WlrLayershell.namespace: "bingux-capture"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            // The guide must never intercept application or top-bar input.
            mask: Region {}
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            readonly property real left: Math.max(0, Math.min(width, root.region.x - modelData.x))
            readonly property real top: Math.max(0, Math.min(height, root.region.y - modelData.y))
            readonly property real right: Math.max(left, Math.min(width, root.region.x + root.region.width - modelData.x))
            readonly property real bottom: Math.max(top, Math.min(height, root.region.y + root.region.height - modelData.y))
            readonly property color dim: "#33000000"
            Rectangle {
                y: Theme.barHeight
                width: shade.width
                height: Math.max(0, shade.top - y)
                color: shade.dim
            }
            Rectangle {
                y: Math.max(Theme.barHeight, shade.top)
                width: shade.left
                height: Math.max(0, shade.bottom - y)
                color: shade.dim
            }
            Rectangle {
                x: shade.right
                y: Math.max(Theme.barHeight, shade.top)
                width: shade.width - x
                height: Math.max(0, shade.bottom - y)
                color: shade.dim
            }
            Rectangle {
                y: Math.max(Theme.barHeight, shade.bottom)
                width: shade.width
                height: shade.height - y
                color: shade.dim
            }
            Rectangle {
                visible: shade.right > shade.left && shade.bottom > shade.top
                x: root.region.x - shade.modelData.x - 1
                y: root.region.y - shade.modelData.y - 1
                width: root.region.width + 2
                height: root.region.height + 2
                color: "transparent"
                border.width: 1
                border.color: "white"
                radius: 2
                // Same markers as selection, but parked just outside the
                // recorded crop and deliberately without pointer handlers.
                Repeater {
                    model: [Qt.point(0, 0), Qt.point(1, 0), Qt.point(0, 1), Qt.point(1, 1), Qt.point(.5, 0), Qt.point(.5, 1), Qt.point(0, .5), Qt.point(1, .5)]
                    CaptureRegionMarker {
                        required property point modelData
                        horizontalEdge: modelData.x === .5
                        verticalEdge: modelData.y === .5
                        x: modelData.x === .5 ? (parent.width - width) / 2 : modelData.x ? parent.width - 1 : 1 - width
                        y: modelData.y === .5 ? (parent.height - height) / 2 : modelData.y ? parent.height - 1 : 1 - height
                    }
                }
            }
        }
    }
}
