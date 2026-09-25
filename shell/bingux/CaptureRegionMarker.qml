import QtQuick

Rectangle {
    property bool horizontalEdge: false
    property bool verticalEdge: false
    property bool active: false
    readonly property bool corner: !horizontalEdge && !verticalEdge
    width: horizontalEdge ? 28 : verticalEdge ? 4 : 10
    height: verticalEdge ? 28 : horizontalEdge ? 4 : 10
    radius: corner ? 5 : 2
    color: active ? Theme.accent : Theme.text
    border.width: corner ? 1 : 0
    border.color: "#66000000"
    Behavior on color {
        ColorAnimation { duration: Theme.reducedMotion ? 0 : 100 }
    }
}
