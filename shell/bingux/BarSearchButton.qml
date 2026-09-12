import QtQuick
import QtQuick.Controls
import Quickshell

AbstractButton {
    id: root
    property var presentation: null
    implicitWidth: presentation?.custom ? face.implicitWidth + Theme.barPrimaryPadding * 2 : Theme.barEdgeHitWidth
    implicitHeight: Theme.barHeight
    activeFocusOnTab: true
    hoverEnabled: true
    Accessible.name: presentation?.label || "Search"
    Accessible.role: Accessible.Button
    Accessible.onPressAction: root.clicked()
    background: BarControlSurface {
        hovered: root.hovered
        pressed: root.down
        focused: root.visualFocus
    }
    contentItem: Item {
        WidgetFace {
            id: face
            anchors.centerIn: parent
            presentation: root.presentation || {
                icon: "system-search-symbolic",
                label: "Search",
                showIcon: true,
                showText: false
            }
        }
    }
    Keys.onReturnPressed: root.clicked()
}
