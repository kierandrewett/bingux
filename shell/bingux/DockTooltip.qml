import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    property string text: ""
    property real centreX: screen ? screen.width / 2 : 0
    implicitWidth: Math.min(320, Math.ceil(metrics.width) + Theme.padding * 2 + 2)
    implicitHeight: Math.ceil(metrics.height) + Theme.gap * 2
    TextMetrics { id: metrics; text: root.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
    visible: false
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "gnoblin-dock-tooltip"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    anchors { bottom: true; left: true }
    margins.bottom: Theme.dockHeight + Theme.padding * 2 + Theme.gap
    margins.left: Math.max(Theme.gap, Math.min(centreX - width / 2, (screen ? screen.width : 1920) - width - Theme.gap))
    mask: Region {}
    Rectangle {
        anchors.fill: parent
        color: Theme.surface
        border.color: Theme.outline
        radius: Theme.radius
        Text {
            id: label
            anchors.fill: parent
            anchors.leftMargin: Theme.padding
            anchors.rightMargin: Theme.padding
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: root.text
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
    }
}
