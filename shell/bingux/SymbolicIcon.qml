import QtQuick
import QtQuick.Effects
import Quickshell.Widgets

Item {
    id: root
    property alias source: image.source
    property int implicitSize: Theme.iconSize
    property color color: Theme.text
    implicitWidth: implicitSize
    implicitHeight: implicitSize
    IconImage {
        id: image
        anchors.fill: parent
        visible: false
    }
    MultiEffect {
        anchors.fill: parent
        source: image
        colorization: 1
        colorizationColor: root.color
        brightness: 1
    }
}
