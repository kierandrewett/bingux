import QtQuick

// The control centre uses one quiet interaction surface everywhere.  The
// state layers are deliberately immediate: delayed hover colours read as a
// flash when moving between the compact controls.
Rectangle {
    id: root
    required property var control
    property color baseColor: "transparent"
    property bool selected: false
    property bool animated: true
    property color hoverColor: Theme.hover
    property color pressedColor: Theme.pressed
    property color focusColor: Theme.accent
    // Idle outlines turn lists into a grid of boxes.  Controls get their
    // boundary from the panel and state layers; only keyboard focus is lined.
    property bool outlined: false
    // Keep the default tighter than a card.  Callers that are genuinely cards
    // can still opt into a larger radius explicitly.
    radius: 8
    color: baseColor
    border.width: outlined ? 1 : 0
    border.color: Theme.outline
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: Theme.selection
        opacity: root.selected ? 0.44 : 0
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.hoverColor
        // Hover only lifts the base very slightly.  Press receives its own
        // layer below, so it stays crisp without a competing "bubble" glow.
        opacity: root.control.enabled && root.control.hovered && !root.control.down ? 0.42 : 0
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.pressedColor
        opacity: root.control.enabled && root.control.down ? 0.72 : 0
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: "transparent"
        border.width: 1
        border.color: root.focusColor
        visible: root.control.enabled && root.control.visualFocus
    }
}
