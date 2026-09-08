import QtQuick

Rectangle {
    property bool selected: false
    property bool hovered: false
    property bool focused: false
    radius: 6
    color: selected ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.34) : hovered ? Theme.surface : "transparent"
    border.width: focused ? 1 : 0
    border.color: Theme.accent
}
