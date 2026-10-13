import QtQuick
import QtQuick.Controls

// Scroll content inside a popup card and keep the indicator in its inset.
Flickable {
    id: root
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    contentWidth: width
    contentHeight: contentItem.childrenRect.height
    property int indicatorPolicy: ScrollBar.AsNeeded
    ScrollBar.vertical: ScrollBar {
        parent: root
        x: root.width - width - 2
        y: 0
        height: root.height
        width: 8
        policy: root.indicatorPolicy
        visible: root.contentHeight > root.height
    }
}
