import QtQuick
import Quickshell
import Quickshell.Wayland

// Shows a large monitor number so the user can match a row in Display settings
// to a physical panel. Input passes through, so it never blocks the screen.
PanelWindow {
    id: root

    // Zero hides the overlay. Otherwise it is the 1-based position in Settings.
    property int number: 0

    color: "transparent"
    focusable: false
    visible: number > 0
    exclusionMode: ExclusionMode.Ignore
    surfaceFormat.opaque: false
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-identify"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }

    Rectangle {
        anchors.centerIn: parent
        width: Math.round(Math.min(root.width, root.height) * 0.3)
        height: width
        radius: width / 2
        color: Theme.popupSurface
        border.width: 2
        border.color: Theme.accent

        Text {
            anchors.centerIn: parent
            text: root.number
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(parent.width * 0.5)
            font.weight: Font.DemiBold
        }
    }
}
