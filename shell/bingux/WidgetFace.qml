import QtQuick
import QtQuick.Layouts
import Quickshell

RowLayout {
    id: root
    required property var presentation
    property url iconSource: presentation?.icon ? Quickshell.iconPath(presentation.icon) : ""
    property bool colouredIcon: false
    property color iconColor: Theme.text
    spacing: Theme.gap
    SymbolicIcon {
        visible: !!root.presentation?.showIcon && !root.colouredIcon
        implicitSize: Theme.iconSize
        source: root.iconSource
        color: root.iconColor
    }
    Image {
        visible: !!root.presentation?.showIcon && root.colouredIcon
        Layout.preferredWidth: Theme.iconSize
        Layout.preferredHeight: Theme.iconSize
        source: root.iconSource
        sourceSize: Qt.size(64, 64)
        fillMode: Image.PreserveAspectFit
    }
    Text {
        objectName: "widgetFaceLabel"
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        visible: !!root.presentation?.showText
        Layout.maximumWidth: 220
        text: root.presentation?.label || ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize
    }
}
