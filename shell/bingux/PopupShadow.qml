import QtQuick
import QtQuick.Effects

// A sibling keeps the shadow outside clipped popup contents and input regions.
Item {
    id: root
    objectName: "popupShadow"
    required property Rectangle surface
    property bool enabledShadow: true
    readonly property var configuredShadow: PopupTransitions.policies["gnoblin-shell-popup"]?.windowShadow
    readonly property var layers: configuredShadow === false || configuredShadow === null ? [] : configuredShadow ? (Array.isArray(configuredShadow) ? configuredShadow : [configuredShadow]) : [
        {
            x: 0,
            y: 18,
            blur: 64,
            spread: 8,
            opacity: .38
        },
        {
            x: 0,
            y: 8,
            blur: 24,
            spread: 2,
            opacity: .26
        },
        {
            x: 0,
            y: 2,
            blur: 6,
            spread: 0,
            opacity: .32
        }
    ]
    parent: surface.parent
    x: surface.x
    y: surface.y
    width: surface.width
    height: surface.height
    z: surface.z - 0.01
    visible: enabledShadow && surface.visible
    opacity: surface.opacity
    scale: surface.scale
    rotation: surface.rotation
    transformOrigin: surface.transformOrigin
    Component.onCompleted: {
        transform = surface.transform;
        PopupTransitions.refresh();
    }
    Repeater {
        model: root.layers
        RectangularShadow {
            required property var modelData
            anchors.fill: parent
            offset: Qt.vector2d(modelData.x ?? 0, modelData.y ?? 4)
            blur: modelData.blur ?? 28
            spread: modelData.spread ?? 4
            radius: root.surface.radius
            color: Qt.alpha(modelData.color ?? "#000000", modelData.opacity ?? .6)
        }
    }
}
