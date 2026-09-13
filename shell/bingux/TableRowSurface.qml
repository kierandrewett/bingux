import QtQuick

Rectangle {
    property bool selected: false
    property bool hovered: false
    property bool focused: false
    radius: 6
    color: selected ? Theme.selection : hovered ? Theme.hover : "transparent"
    border.width: focused ? 1 : 0
    border.color: Theme.accent
}
