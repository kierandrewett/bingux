import QtQuick
import QtQuick.Controls as Controls

Controls.ToolTip {
    id: root
    property int maximumWidth: 320
    property bool wrapText: true
    property int revealDuration: 0
    onAboutToShow: revealDuration = Theme.beginTooltip(root)
    onClosed: Theme.endTooltip(root)
    Component.onDestruction: Theme.endTooltip(root)
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: root.revealDuration; easing.type: Easing.OutCubic }
    }
    exit: Transition {}
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
