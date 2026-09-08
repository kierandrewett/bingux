import QtQuick

// Inset the background while retaining the full-height pointer target.
Rectangle {
    property bool hovered: false
    property bool pressed: false
    property bool selected: false
    property bool focused: false
    anchors.fill: parent
    anchors.margins: Theme.barControlInset
    radius: Theme.barControlRadius
    color: pressed ? Theme.pressed : hovered || focused ? Theme.hover : selected ? Theme.elevated : "transparent"
}
