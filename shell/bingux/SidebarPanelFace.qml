import QtQuick
import "DesktopLayout.js" as DesktopLayout

// The retained native panel and its sample preview share the same frame.
Item {
    id: root
    required property string widgetId
    readonly property var spec: DesktopLayout.widget(widgetId)
    property bool inlinePanel: false
    property bool showHeader: true
    property real panelHeight: 360
    property bool fitContent: false
    property var presentation: null
    property var barWindow: null
    property bool selected: false
    readonly property alias panelBody: body
    signal clicked
    implicitWidth: inlinePanel ? 320 : launcher.implicitWidth
    implicitHeight: inlinePanel ? Math.min(480, panelHeight + (fitContent && showHeader ? heading.height + Theme.gap * 3 : 0)) : Theme.barHeight
    IconButton {
        id: launcher
        objectName: "sidebar-panel-launcher-" + root.widgetId
        anchors.fill: parent
        visible: !root.inlinePanel
        iconName: root.spec.icon
        label: root.spec.label
        presentation: root.presentation
        barStyle: true
        barWindow: root.barWindow
        highlighted: root.selected
        onClicked: root.clicked()
    }
    WidgetFace {
        id: heading
        visible: root.inlinePanel && root.showHeader
        x: Theme.gap
        y: Theme.gap
        presentation: root.presentation
    }
    Item {
        id: body
        visible: root.inlinePanel
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: root.showHeader ? heading.bottom : parent.top
            margins: root.showHeader ? Theme.gap : 0
        }
        clip: true
    }
}
