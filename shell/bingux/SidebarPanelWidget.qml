import QtQuick
import QtQuick.Layouts
import Quickshell
import "DesktopLayout.js" as DesktopLayout

SidebarPanelFace {
    id: root
    required property var sidebar
    required property var widgetLayout
    signal editRequested(string widgetId, var item, var window)
    signal opening(var popup)
    readonly property string container: DesktopEditing.desktop.layout ? DesktopLayout.placement(DesktopEditing.desktop, widgetId) : "sidebar"
    readonly property bool sidebarMode: container === "sidebar"
    inlinePanel: container === "control-centre"
    readonly property bool placed: container !== "" && !sidebarMode
    readonly property var panelItem: sidebar.panelFor(widgetId)
    panelHeight: widgetId === "media" ? Math.max(160, panelItem?.contentHeight || 0) : 360
    fitContent: widgetId === "media"
    barWindow: widgetLayout ? widgetLayout.windowFor(root) : null
    presentation: DesktopLayout.presentation(DesktopEditing.desktop, widgetId, container, spec.label, spec.icon, true, inlinePanel)
    readonly property Item contentHost: sidebarMode ? sidebar.panelHost : inlinePanel ? panelBody : panelPopup.body
    readonly property bool contentVisible: sidebarMode ? sidebar.contentType === widgetId : inlinePanel ? visible : panelPopup.visible
    readonly property alias popup: panelPopup
    parent: placed && widgetLayout ? widgetLayout.hostFor(root) : null
    visible: placed
    selected: panelPopup.visible
    onClicked: panelPopup.visible = !panelPopup.visible
    Layout.fillWidth: inlinePanel
    Layout.column: widgetLayout ? widgetLayout.controlColumn(root) : 0
    Layout.row: widgetLayout ? widgetLayout.controlRow(root) : 0
    onContainerChanged: panelPopup.visible = false
    WidgetEditHandle {
        control: root
        widgetId: root.widgetId
        previewSource: false
        onRequested: (id, item) => root.editRequested(id, item, root.barWindow)
    }
    ShellPopup {
        id: panelPopup
        screen: root.barWindow ? root.barWindow.screen : root.sidebar.screen
        anchorWindow: root.barWindow
        anchorItem: root
        popupWidth: 400
        popupHeight: Math.min(root.widgetId === "media" ? root.panelHeight + contentPadding * 2 : 480, Math.max(160, height - Theme.barHeight - Theme.dockExclusiveHeight - Theme.gap * 4))
        onVisibleChanged: if (visible) {
            root.opening(panelPopup);
            Qt.callLater(() => root.sidebar.focusPanel(root.widgetId));
        }
    }
}
