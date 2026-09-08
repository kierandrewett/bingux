import QtQuick

// Follow the panel as a sibling, so clipping cannot cut off the outer stroke.
// This item's geometry stays identical to the panel; only its pixels extend.
Item {
    id: root
    required property Rectangle surface
    parent: surface.parent
    x: surface.x
    y: surface.y
    width: surface.width
    height: surface.height
    z: surface.z + 0.01
    visible: surface.visible && surface.border.width > 0
    opacity: surface.opacity
    scale: surface.scale
    rotation: surface.rotation
    transformOrigin: surface.transformOrigin
    Component.onCompleted: transform = surface.transform
    Rectangle {
        anchors.fill: parent
        anchors.margins: -1
        radius: root.surface.radius + 1
        color: "transparent"
        border.width: 1
        border.color: Theme.panelOuterOutline
    }
}
