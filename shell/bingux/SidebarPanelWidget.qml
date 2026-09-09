import QtQuick
import QtQuick.Layouts
import Quickshell
import "DesktopLayout.js" as DesktopLayout

Item {
    id: root
    required property string widgetId
    required property var sidebar
    required property var widgetLayout
    signal editRequested(string widgetId, var item, var window)
    signal opening(var popup)
    readonly property var spec: DesktopLayout.widget(widgetId)
    readonly property string container: DesktopEditing.desktop.layout ? DesktopLayout.placement(DesktopEditing.desktop, widgetId) : "sidebar"
    readonly property bool sidebarMode: container === "sidebar"
    readonly property bool inlinePanel: container === "control-centre"
    readonly property bool placed: container !== "" && !sidebarMode
    readonly property var panelItem: sidebar.panelFor(widgetId)
    readonly property real panelHeight: widgetId === "media" ? Math.max(160, panelItem?.contentHeight || 0) : 360
    readonly property var barWindow: widgetLayout ? widgetLayout.windowFor(root) : null
    readonly property var presentation: DesktopLayout.presentation(DesktopEditing.desktop, widgetId, container, spec.label, spec.icon, true, inlinePanel)
    readonly property Item contentHost: sidebarMode ? sidebar.panelHost : inlinePanel ? inlineBody : panelPopup.body
    readonly property bool contentVisible: sidebarMode ? sidebar.contentType === widgetId : inlinePanel ? visible : panelPopup.visible
    readonly property alias popup: panelPopup
    parent: placed && widgetLayout ? widgetLayout.hostFor(root) : null
    visible: placed
    implicitWidth: inlinePanel ? 320 : launcher.implicitWidth
    implicitHeight: inlinePanel ? Math.min(480, panelHeight) : Theme.barHeight
    Layout.fillWidth: inlinePanel
    Layout.column: widgetLayout ? widgetLayout.controlColumn(root) : 0
    Layout.row: widgetLayout ? widgetLayout.controlRow(root) : 0
    onContainerChanged: panelPopup.visible = false
    WidgetEditHandle { control: root; widgetId: root.widgetId; previewSource: false; onRequested: (id, item) => root.editRequested(id, item, root.barWindow) }
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
        highlighted: panelPopup.visible
        onClicked: panelPopup.visible = !panelPopup.visible
    }
    WidgetFace {
        id: heading
        visible: root.inlinePanel
        x: Theme.gap; y: Theme.gap
        presentation: root.presentation
    }
    Item {
        id: inlineBody
        visible: root.inlinePanel
        anchors { left: parent.left; right: parent.right; top: heading.bottom; bottom: parent.bottom; margins: Theme.gap }
        clip: true
    }
    ShellPopup {
        id: panelPopup
        screen: root.barWindow ? root.barWindow.screen : root.sidebar.screen
        anchorWindow: root.barWindow
        anchorItem: root
        popupWidth: 400
        popupHeight: Math.min(root.widgetId === "media" ? root.panelHeight + contentPadding * 2 : 480,
            Math.max(160, height - Theme.barHeight - Theme.dockExclusiveHeight - Theme.gap * 4))
        onVisibleChanged: if (visible) { root.opening(panelPopup); Qt.callLater(() => root.sidebar.focusPanel(root.widgetId)); }
    }
}
