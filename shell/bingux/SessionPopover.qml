import QtQuick

// Small frosted panel that opens above a corner control on the lock screen
// and greeter. Content is a single child item sized to the panel width; the
// panel grows to fit it. Closed, it is invisible and takes no input.
Item {
    id: root

    property bool open: false
    property Item backdrop: null
    default property alias content: body.data
    readonly property int padding: 6

    implicitHeight: body.childrenRect.height + padding * 2
    visible: opacity > 0
    enabled: open
    z: 10
    opacity: open ? 1 : 0
    scale: open ? 1 : 0.96
    transformOrigin: Item.Bottom
    Behavior on opacity {
        NumberAnimation {
            duration: root.open ? Theme.popupOpenMotion : Theme.popupCloseMotion
        }
    }
    Behavior on scale {
        NumberAnimation {
            duration: root.open ? Theme.popupOpenMotion : Theme.popupCloseMotion
            easing.type: Easing.OutCubic
        }
    }

    // A dark tint over the blur: white rows stay readable on a bright sky,
    // which a white tint cannot promise.
    SessionGlass {
        anchors.fill: parent
        backdrop: root.backdrop
        radius: 14
        tint: Qt.rgba(0, 0, 0, 0.32)
        outline: Qt.rgba(1, 1, 1, 0.16)
    }
    // Clicks inside the panel must not reach the dismiss area behind it.
    MouseArea {
        anchors.fill: parent
    }
    Item {
        id: body
        anchors.fill: parent
        anchors.margins: root.padding
    }
}
