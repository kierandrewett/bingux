import QtQuick
import QtQuick.Controls as Controls

Controls.ToolTip {
    id: root
    property int maximumWidth: 320
    property bool wrapText: true
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
