import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root
    default property alias contents: row.data
    property alias spacing: row.spacing
    implicitWidth: row.implicitWidth + Theme.padding * 2
    implicitHeight: Theme.controlHeight
    radius: height / 2
    color: Theme.surface
    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Theme.gap
    }
}
