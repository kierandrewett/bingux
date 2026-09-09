import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root
    default property alias rows: content.data
    property bool separators: true
    readonly property var visibleRows: content.children.filter(item => item.visible && item.height > 1)
    Layout.fillWidth: true
    implicitHeight: content.implicitHeight + Theme.spaceSmall * 2
    radius: Theme.radius
    color: Theme.settingsSurface
    ColumnLayout {
        id: content
        x: Theme.spaceSmall; y: Theme.spaceSmall
        width: parent.width - Theme.spaceSmall * 2
        spacing: root.separators ? 1 : 0
    }
    Repeater {
        model: root.separators ? root.visibleRows : []
        Rectangle {
            required property var modelData
            required property int index
            visible: index < root.visibleRows.length - 1
            x: Theme.spaceSmall
            y: Theme.spaceSmall + modelData.y + modelData.height
            width: root.width - x * 2
            height: 1
            color: Theme.settingsSeparator
        }
    }
}
