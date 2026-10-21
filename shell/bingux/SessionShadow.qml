import QtQuick
import QtQuick.Effects

// Soft drop shadow for static content on the lock screen and greeter (the
// clock, the account name, the avatar). The content is rendered through a
// padded ShaderEffectSource so the shadow has room to spread; a layer.effect
// or MultiEffect's own padding clips it to the item bounds in Quickshell's
// Qt build. Software rendering shows the content plain.
Item {
    id: root

    default property alias content: holder.data
    property int padding: 40
    property real blur: 0.8
    property real offsetY: 4
    property color colour: Qt.rgba(0, 0, 0, 0.4)
    readonly property bool active: !Theme.softwareRendering

    implicitWidth: holder.childrenRect.width
    implicitHeight: holder.childrenRect.height

    Item {
        id: holder
        anchors.fill: parent
    }
    ShaderEffectSource {
        id: source
        sourceItem: holder
        sourceRect: Qt.rect(-root.padding, -root.padding, holder.width + root.padding * 2, holder.height + root.padding * 2)
        hideSource: root.active
        visible: false
        live: true
    }
    MultiEffect {
        visible: root.active
        x: -root.padding
        y: -root.padding
        width: holder.width + root.padding * 2
        height: holder.height + root.padding * 2
        source: source
        autoPaddingEnabled: false
        blurMax: 48
        shadowEnabled: true
        shadowColor: root.colour
        shadowOpacity: 1
        shadowBlur: root.blur
        shadowVerticalOffset: root.offsetY
    }
}
