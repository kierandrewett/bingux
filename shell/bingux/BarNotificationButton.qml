import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

AbstractButton {
    id: root
    property int count: 0
    property var presentation: null
    property var barWindow: null
    property bool selected: false
    implicitWidth: face.implicitWidth + Theme.barEdgeHitWidth - Theme.iconSize
    implicitHeight: Theme.barHeight
    hoverEnabled: true
    Accessible.name: "Notifications"
    Accessible.description: count + " notifications"
    function playArchive() { badge.playArchive(); }
    contentItem: Item {
        RowLayout {
            id: face; anchors.centerIn: parent; spacing: Theme.gap
            WidgetFace { visible: !!root.presentation?.custom; presentation: root.presentation }
            NotificationIndicator { id: badge; count: root.count }
        }
    }
    BarTooltip {
        reorderable: true; anchorItem: root; barWindow: root.barWindow; requested: root.hovered
        text: root.count > 0 ? "Notifications · " + root.count : "No notifications"
    }
    background: BarControlSurface {
        hovered: root.hovered
        pressed: root.down
        focused: root.activeFocus
        selected: root.selected
    }
}
