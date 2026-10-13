import QtQuick
import QtQuick.Effects
import Quickshell.Widgets

Item {
    id: root
    property url source: ""
    property int implicitSize: Theme.iconSize
    property color color: Theme.text
    readonly property url resolvedSource: AdwaitaIcons.source(source)
    implicitWidth: implicitSize
    implicitHeight: implicitSize
    IconImage {
        id: image
        anchors.fill: parent
        source: root.resolvedSource
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
