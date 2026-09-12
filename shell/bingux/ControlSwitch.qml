import QtQuick
import QtQuick.Controls

Switch {
    id: root
    implicitWidth: 36
    implicitHeight: 30
    padding: 0
    hoverEnabled: true
    indicator: Rectangle {
        y: (root.height - height) / 2
        width: 36
        height: 20
        radius: 10
        // A defined track and a light thumb make this read as a switch rather
        // than another rounded button in the grid.
        color: root.checked ? Theme.accent : Theme.elevated
        opacity: root.enabled ? 1 : 0.45
        border.width: root.visualFocus || !root.checked ? 1 : 0
        border.color: root.visualFocus ? Theme.text : Theme.outline
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: "white"
            opacity: root.enabled && root.hovered && !root.down ? 0.08 : 0
        }
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: "black"
            opacity: root.enabled && root.down ? 0.16 : 0
        }
        Rectangle {
            x: root.checked ? parent.width - width - 2 : 2
            y: 2
            width: root.down ? 18 : 16
            height: 16
            radius: 8
            Behavior on width {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : 80
                    easing.type: Easing.OutCubic
                }
            }
            color: root.checked ? "#ffffff" : Theme.text
            Behavior on x {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : 100
                    easing.type: Easing.OutCubic
                }
            }
        }
    }
    contentItem: Item {}
}
