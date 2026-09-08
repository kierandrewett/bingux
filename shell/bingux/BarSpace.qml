import QtQuick
import QtQuick.Layouts

Item {
    id: root
    property bool flexible: false
    property bool editing: DesktopEditing.active
    property real gapSize: 20
    implicitWidth: flexible ? 24 : gapSize
    implicitHeight: Theme.barHeight
    Layout.minimumWidth: flexible ? 8 : gapSize
    Layout.maximumWidth: flexible ? Infinity : gapSize
    Layout.fillWidth: flexible
    Rectangle {
        anchors.fill: parent
        anchors.margins: 5
        visible: root.editing
        radius: 3
        color: Qt.rgba(Theme.text.r, Theme.text.g, Theme.text.b, 0.04)
        border.width: 1
        border.color: Theme.outline
        Row {
            anchors.centerIn: parent
            spacing: 3
            Repeater {
                model: root.flexible ? 3 : 1
                Rectangle { width: 2; height: 7; radius: 1; color: Theme.muted }
            }
        }
    }
}
