import QtQuick
import QtQuick.Controls as Controls

Controls.ToolTip {
    id: root
    property int maximumWidth: 320
    property bool wrapText: true
    onAboutToShow: Theme.beginTooltip(root)
    onClosed: Theme.endTooltip(root)
    Component.onDestruction: Theme.endTooltip(root)
    readonly property int revealDuration: Theme.tooltipMotion
    enter: Transition {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: root.revealDuration
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            property: "scale"
            from: Theme.tooltipHiddenScale
            to: 1
            duration: root.revealDuration
            easing.type: Easing.OutCubic
        }
    }
    exit: Transition {
        NumberAnimation {
            property: "opacity"
            to: 0
            duration: Theme.tooltipMotion
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            property: "scale"
            to: Theme.tooltipHiddenScale
            duration: Theme.tooltipMotion
            easing.type: Easing.OutCubic
        }
    }
    delay: Theme.tooltipDelay
    timeout: 8000
    padding: 0
    implicitWidth: contentItem.implicitWidth
    implicitHeight: contentItem.implicitHeight
    background: Item {}
    contentItem: TooltipBubble {
        text: root.text
        maximumWidth: root.maximumWidth
        wrapText: root.wrapText
    }
}
