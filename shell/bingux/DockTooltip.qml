import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    property string text: ""
    property real centreX: screen ? screen.width / 2 : 0
    // Distance from the screen bottom to the dock's actual top edge.
    property real anchorBottom: Theme.dockHeight + Theme.padding
    implicitWidth: surface.implicitWidth
    implicitHeight: surface.implicitHeight
    visible: false
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "gnoblin-dock-tooltip"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    anchors { bottom: true; left: true }
    margins.bottom: Math.max(Theme.gap, Math.min(anchorBottom + Theme.spaceSmall, (screen ? screen.height : 1080) - height - Theme.gap))
    margins.left: Math.max(Theme.gap, Math.min(centreX - width / 2, (screen ? screen.width : 1920) - width - Theme.gap))
    mask: Region {}
    TooltipBubble {
        id: surface
        anchors.fill: parent
        text: root.text
        opacity: root.visible ? 1 : 0

        Behavior on opacity {
            NumberAnimation { duration: 160; easing.type: Easing.OutCubic }
        }

    }
}
