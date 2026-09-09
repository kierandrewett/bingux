import QtQuick
import QtQuick.Controls
import Quickshell

AbstractButton {
    id: root
    property var presentation: null
    property var barWindow: null
    property bool selected: false
    function activateInEditor() { clicked(); }
    implicitWidth: presentation?.custom ? face.implicitWidth + Theme.barPrimaryPadding * 2 : Theme.barEdgeHitWidth
    implicitHeight: Theme.barHeight
    hoverEnabled: true
    activeFocusOnTab: true
    Accessible.name: "More status controls"
    Keys.onReturnPressed: clicked()
    background: BarControlSurface { hovered: root.hovered; pressed: root.down; selected: root.selected; focused: root.visualFocus }
    contentItem: Item {
        WidgetFace { id: face; anchors.centerIn: parent; visible: !!root.presentation?.custom; presentation: root.presentation }
        SymbolicIcon { anchors.centerIn: parent; visible: !root.presentation?.custom; implicitSize: Theme.iconSize; source: Quickshell.iconPath("view-more-symbolic") }
    }
    BarTooltip { reorderable: true; anchorItem: root; barWindow: root.barWindow; requested: root.hovered; text: root.Accessible.name }
}
