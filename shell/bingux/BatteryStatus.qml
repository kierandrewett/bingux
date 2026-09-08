import QtQuick
import QtQuick.Layouts
import Quickshell

Item {
    id: root
    property bool available: false
    property string summary: ""
    property bool barLayout: false
    property var barWindow: null
    property var presentation: null
    readonly property string label: available ? summary.replace(/^Battery /, "").replace(" percent", "%").replace(/,.*$/, "") : "No battery"
    readonly property bool customPresentation: !!presentation?.custom
    implicitWidth: customPresentation ? face.implicitWidth : nativeFace.implicitWidth
    implicitHeight: barLayout ? Theme.barHeight : customPresentation ? face.implicitHeight : nativeFace.implicitHeight
    Accessible.name: available ? summary : label
    RowLayout {
        id: nativeFace
        anchors.centerIn: parent
        visible: !root.customPresentation
        spacing: 8
        SymbolicIcon { implicitSize: 16; color: Theme.muted; source: Quickshell.iconPath("battery-good-symbolic") }
        Text { text: root.label; Accessible.name: root.Accessible.name; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
    }
    WidgetFace { id: face; anchors.centerIn: parent; visible: root.customPresentation; presentation: root.presentation; iconColor: Theme.muted }
    HoverHandler { id: hover }
    BarTooltip { anchorItem: root; barWindow: root.barWindow; requested: root.barLayout && root.visible && hover.hovered && !DesktopEditing.active; text: root.Accessible.name }
}
