import QtQuick
import QtQuick.Layouts

Item {
    id: root
    default property alias contents: row.data
    property alias spacing: row.spacing
    property color color: hover.hovered || activeFocus ? Theme.surface : "transparent"
    implicitWidth: row.implicitWidth + Theme.paddingLarge * 2
    implicitHeight: Theme.barHeight
    HoverHandler { id: hover }
    Rectangle {
        anchors.centerIn: parent
        width: parent.width
        height: Theme.controlHeight
        radius: height / 2
        color: root.color
    }
    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Theme.gap
    }
}
