import QtQuick
import QtQuick.Layouts
import Quickshell

Item {
    id: root
    default property alias contents: row.data
    property alias spacing: row.spacing
    property bool interactive: false
    property bool hovered: interactive && hover.hovered
    property bool pressed: false
    property bool selected: false
    property int horizontalPadding: Theme.barControlPadding
    property var presentation: null
    readonly property bool customPresentation: !!presentation?.custom
    implicitWidth: (customPresentation ? customFace.implicitWidth : row.implicitWidth) + horizontalPadding * 2
    implicitHeight: Theme.barHeight
    HoverHandler { id: hover; enabled: root.interactive }
    BarControlSurface {
        hovered: root.hovered
        pressed: root.pressed
        selected: root.selected
        focused: root.activeFocus
    }
    RowLayout {
        id: row
        visible: !root.customPresentation
        anchors.centerIn: parent
        spacing: Theme.gap
    }
    WidgetFace {
        id: customFace
        visible: root.customPresentation
        anchors.centerIn: parent
        presentation: root.presentation
    }
}
