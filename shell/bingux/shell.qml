import Quickshell
import Quickshell.Wayland
import QtQuick

ShellRoot {
    id: root

    readonly property int topBarHeight: 36

    PanelWindow {
        id: topBar

        anchors {
            top: true
            left: true
            right: true
        }

        exclusiveZone: root.topBarHeight
        implicitHeight: root.topBarHeight
        color: "#171a21"

        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-top-bar"

        Text {
            anchors.centerIn: parent
            color: "#f5f7fa"
            font.pixelSize: 14
            text: "Bingux"
        }
    }
}
