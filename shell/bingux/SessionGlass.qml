import QtQuick
import QtQuick.Effects
import Quickshell.Widgets

// Frosted surface for the lock screen and greeter: the wallpaper behind this
// item, blurred and tinted, under a rounded outline. The blur samples the
// backdrop item directly so it stays correct when the host moves.
//
// Software rendering has no shader blur; the surface falls back to the plain
// translucent tint, which is what the rest of the shell does for its glass.
Item {
    id: root

    // The wallpaper image to sample. Null disables the blur.
    property Item backdrop: null
    property real radius: height / 2
    property color tint: Qt.rgba(1, 1, 1, 0.16)
    property color outline: Qt.rgba(1, 1, 1, 0.22)
    property int outlineWidth: 1
    readonly property bool blurred: backdrop !== null && !Theme.softwareRendering

    // Re-evaluated whenever this item or any ancestor between it and the
    // backdrop moves: the layout animations read these in the binding.
    readonly property rect region: {
        if (!root.backdrop)
            return Qt.rect(0, 0, 0, 0);
        let item = root;
        // Touch every ancestor's geometry so the binding tracks motion.
        while (item && item !== root.backdrop.parent) {
            const _ = [item.x, item.y, item.width, item.height, item.scale];
            item = item.parent;
        }
        return root.mapToItem(root.backdrop, 0, 0, root.width, root.height);
    }

    ClippingRectangle {
        anchors.fill: parent
        radius: root.radius
        color: "transparent"
        visible: root.blurred
        ShaderEffectSource {
            id: crop
            anchors.fill: parent
            sourceItem: root.backdrop
            sourceRect: root.region
            visible: false
            live: true
        }
        MultiEffect {
            anchors.fill: parent
            source: crop
            blurEnabled: true
            blur: 1
            blurMax: 48
            blurMultiplier: 1.2
            autoPaddingEnabled: false
            saturation: 0.15
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.tint
        border.width: root.outlineWidth
        border.color: root.outline
    }
}
