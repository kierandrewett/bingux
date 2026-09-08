import QtQuick
import QtQuick.Controls
import Quickshell

AbstractButton {
    id: root
    implicitWidth: Theme.barEdgeHitWidth
    implicitHeight: Theme.barHeight
    activeFocusOnTab: true
    hoverEnabled: true
    Accessible.name: "Search"
    Accessible.role: Accessible.Button
    Accessible.onPressAction: root.clicked()
    background: BarControlSurface {
        hovered: root.hovered
        pressed: root.down
        focused: root.visualFocus
    }
    contentItem: Item {
        SymbolicIcon {
            anchors.centerIn: parent
            implicitSize: Theme.iconSize
            source: Quickshell.iconPath("system-search-symbolic")
        }
    }
    Keys.onReturnPressed: root.clicked()
}
