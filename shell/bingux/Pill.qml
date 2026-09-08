import QtQuick
import QtQuick.Layouts

Item {
    id: root
    default property alias contents: row.data
    property alias spacing: row.spacing
    property bool interactive: false
    property bool hovered: interactive && hover.hovered
    property bool pressed: false
    property bool selected: false
    property int horizontalPadding: Theme.barControlPadding
    implicitWidth: row.implicitWidth + horizontalPadding * 2
    implicitHeight: Theme.barHeight
    HoverHandler { id: hover; enabled: root.interactive }
    BarControlSurface {
        hovered: root.hovered
        pressed: root.pressed
        selected: root.selected
        focused: root.activeFocus
    }
    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Theme.gap
    }
}
