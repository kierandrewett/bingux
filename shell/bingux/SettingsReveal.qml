import QtQuick
import QtQuick.Layouts

Item {
    id: root
    default property alias content: body.data
    property bool expanded: false
    property real progress: expanded ? 1 : 0
    Layout.fillWidth: true
    implicitHeight: body.implicitHeight * progress
    visible: progress > 0
    opacity: progress
    clip: true
    Behavior on progress {
        NumberAnimation { duration: Theme.reducedMotion ? 0 : 160; easing.type: Easing.OutCubic }
    }
    ColumnLayout {
        id: body
        width: parent.width
        spacing: 0
    }
}
