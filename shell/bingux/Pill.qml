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
    property bool panelLayout: false
    readonly property bool customPresentation: !!presentation?.custom
    implicitWidth: (customPresentation ? customFace.implicitWidth : row.implicitWidth) + horizontalPadding * 2
    implicitHeight: panelLayout && !customPresentation
        ? Math.max(Theme.barHeight, row.implicitHeight + Theme.gap * 2) : Theme.barHeight
    width: panelLayout && parent ? parent.width : implicitWidth
    Layout.fillWidth: panelLayout
    Layout.maximumWidth: panelLayout && parent ? parent.width : Infinity
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
        width: root.panelLayout ? Math.max(0, root.width - root.horizontalPadding * 2) : implicitWidth
        spacing: Theme.gap
    }
    WidgetFace {
        id: customFace
        visible: root.customPresentation
        anchors.centerIn: parent
        width: root.panelLayout ? Math.min(implicitWidth, Math.max(0, root.width - root.horizontalPadding * 2)) : implicitWidth
        presentation: root.presentation
    }
}
