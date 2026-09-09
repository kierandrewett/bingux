import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Services.Mpris
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

ShellPopup {
    id: root
    keepWindowAlive: DesktopEditing.editor !== null
    readonly property var groupedLayout: controls.groupedLayout
    function snapshotLayout() { return JSON.parse(JSON.stringify(BinguxPreferences.data.desktop.controlLayout || ControlLayout.defaults())); }
    function sectionRow(id) { return ControlLayout.position(groupedLayout, "control-centre", id); }
    function groupPosition(group, id) { return ControlLayout.position(groupedLayout, group, id); }
    required property var indicators
    signal widgetEditRequested(string widgetId, var control)
    signal customiseRequested()
    property var widgetLayout: null
    readonly property alias widgetHost: controls
    readonly property var externalEntries: widgetLayout ? widgetLayout.defaultControls.map(item => ({id: widgetLayout.nameFor(item), item})).filter(entry => ControlLayout.isExternal(entry.id) && ControlLayout.contains(groupedLayout, "control-centre", entry.id)) : []
    property Item movedAnchor: null
    readonly property var actionWidgets: controls.actionWidgets
    readonly property var movableWidgets: controls.movableWidgets
    readonly property var widgetOrder: controls.widgetOrder
    function controlVisible(name) { return controls.controlVisible(name); }
    function controlSpan(name) { return controls.controlSpan(name); }
    function controlCell(name) { return controls.controlCell(name); }
    readonly property var controlChoices: extrasView.choices
    readonly property var deviceControls: detailView
    property var services: ControlCentreServices
    readonly property bool extraPage: ["vpn", "power", "customise"].includes(detailPage)
    readonly property var activeDetailView: extraPage ? extrasView : detailView
    readonly property var connectedVpns: services.vpns.filter(vpn => vpn.connected)
    readonly property bool showVpn: services.showControl("vpn") && services.vpns.length > 0
    Binding { target: root.services; property: "active"; value: root.visible || root.movableWidgets.some(item => item.placed) }
    property bool detailOpen: false
    property string detailPage: "network"
    property real detailProgress: detailOpen ? 1 : 0
    Behavior on detailProgress { NumberAnimation { duration: Theme.reducedMotion ? 0 : 160; easing.type: Easing.OutCubic } }
    property var detailTrigger: null
    property bool pendingDetailFocus: false
    property bool pendingOverviewFocus: false
    function openDetail(page, trigger, audioTab) {
        let widget = trigger;
        while (widget && !widget.widgetId) widget = widget.parent;
        if (widget && widget.Window.window !== root.body.Window.window) { movedAnchor = widget; visible = true; }
        if (page === "audio") detailView.audioTab = audioTab || "output";
        pendingOverviewFocus = false;
        detailTrigger = trigger || null;
        pendingDetailFocus = !!(trigger && trigger.visualFocus);
        detailPage = page;
        detailOpen = true;
    }
    Timer {
        interval: 16
        repeat: true
        running: root.detailOpen && root.pendingDetailFocus
        onTriggered: if (root.activeDetailView.visible) {
            root.activeDetailView.focusBack(Qt.TabFocusReason);
            root.pendingDetailFocus = false;
        }
    }
    function closeDetail() {
        const keyboard = activeDetailView.keyboardNavigation;
        detailOpen = false;
        pendingOverviewFocus = keyboard && !!detailTrigger;
    }
    Timer {
        interval: 16
        repeat: true
        running: root.pendingOverviewFocus && !root.detailOpen
        onTriggered: {
            if (!root.visible) root.pendingOverviewFocus = false;
            else if (controls.visible) {
                if (root.detailTrigger) root.detailTrigger.forceActiveFocus(Qt.TabFocusReason);
                root.pendingOverviewFocus = false;
            }
        }
    }
    Connections { target: root; function onVisibleChanged() { if (!root.visible) { root.detailOpen = false; root.movedAnchor = null; } } }
    property var bluetoothAdapter: Bluetooth.defaultAdapter
    readonly property var microphone: indicators.audioSource || null
    readonly property bool microphoneAvailable: microphone !== null && microphone.ready && microphone.audio !== null
    property var mediaPlayers: Mpris.players.values
    property var selectedMediaPlayer: null
    property var mediaPlayer: mediaPlayers.indexOf(selectedMediaPlayer) >= 0 ? selectedMediaPlayer
        : mediaPlayers.find(player => player.isPlaying) || mediaPlayers[0] || null
    popupWidth: Theme.notificationWidth + contentPadding * 2
    property real dockSafeInset: Theme.dockExclusiveHeight
    readonly property real dockSafeBottom: height - dockSafeInset - Theme.gap
    readonly property real editorTop: Theme.barHeight + 40 + (DesktopEditing.editor?.topInset || 0)
    readonly property real maximumPopupHeight: Math.max(0, Math.min(height * 0.8, DesktopEditing.active ? dockSafeBottom - editorTop : anchorAbove ? anchorTop - Theme.barHeight - Theme.gap : dockSafeBottom - belowAnchorY))
    property real controlsHeight: Math.min(detailOpen ? activeDetailView.implicitHeight : Math.max(96, controls.implicitHeight),
        Math.max(0, maximumPopupHeight - contentPadding * 2))
    Behavior on controlsHeight {
        enabled: root.visible && root.revealScale === 1
        NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
    }
    popupHeight: Math.ceil(controlsHeight) + contentPadding * 2
    contentPadding: 16
    cornerRadius: Theme.cardRadius
    surfaceColor: Theme.popupSurface
    preferredX: DesktopEditing.active ? width - popupWidth - DesktopEditing.editor.rightInset - 24 : anchorItem ? anchorPosition.x - popupWidth : width - popupWidth - Theme.padding

    preferredY: DesktopEditing.active ? editorTop : anchorItem ? (anchorAbove ? anchorTop - popupHeight - Theme.gap : anchorPosition.y + Theme.gap) : Theme.barHeight + Theme.gap
    dismissOnOutsideClick: !DesktopEditing.active
    keyboardInteractive: !DesktopEditing.active
    NativeEditSurface {
        parent: root.body.parent; anchors.fill: parent; window: root.nativeWindow; zoneName: "control-centre"; vertical: ControlLayout.groupFor(DesktopEditing.editor?.draggedId || "") !== "controls-header"
        geometryItem: {
            const group = ControlLayout.groupFor(DesktopEditing.editor?.draggedId || "");
            return group === "controls-header" && controls.headerGroup.visible && !controls.headerGroup.placed ? controls.headerGroup
                : group === "controls-audio" && controls.audioGroup.visible && !controls.audioGroup.placed ? controls.audioGroup : root.body.parent;
        }
        entries: root.movableWidgets.filter(item => !item.barLayout && !item.memberEntries).map(item => ({id: item.widgetId, item})).concat(root.movableWidgets.filter(item => !item.barLayout && item.memberEntries).map(item => ({id: item.widgetId, item}))).concat(root.externalEntries).filter(entry => entry.item.Window.window === root.body.Window.window)
    }

    function settings(panel) {
        Quickshell.execDetached(["gnome-control-center", panel]);
        visible = false;
    }

    Item {
        id: controlDeck
        objectName: "controlLiveWidgets"
        enabled: !DesktopEditing.active
        opacity: DesktopEditing.active ? 0.68 : 1
        width: parent.width
        height: root.controlsHeight
        Text {
            anchors.centerIn: parent
            visible: !root.detailOpen && controls.implicitHeight === 0
            text: DesktopEditing.active ? "Drop widgets here" : "No controls here"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
    Flickable {
        id: overview
        anchors.fill: parent
        contentHeight: controls.implicitHeight
        contentWidth: width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {
            parent: controlDeck
            x: overview.width + 4
            y: 0
            height: overview.height
            width: 8
            visible: overview.visible
        }
        transform: Translate { x: -root.detailProgress * 12 }
        opacity: 1 - root.detailProgress
        visible: root.detailProgress < 1
        enabled: !root.detailOpen
    ControlCentreOverview {
        id: controls
        objectName: "controlOverviewRows"
        width: overview.width
        indicators: root.indicators
        services: root.services
        widgetLayout: root.widgetLayout
        barWindow: root.nativeWindow
        active: root.visible
        bluetoothAdapter: root.bluetoothAdapter
        mediaPlayers: root.mediaPlayers
        mediaPlayer: root.mediaPlayer
        onSettingsRequested: panel => root.settings(panel)
        onDetailRequested: (page, trigger, audioTab) => root.openDetail(page, trigger, audioTab)
        onLockRequested: { Quickshell.execDetached(["loginctl", "lock-session"]); root.visible = false; }
        onWidgetEditRequested: (id, item) => root.widgetEditRequested(id, item)
        onCustomiseRequested: root.customiseRequested()
        onPlayerSelected: player => root.selectedMediaPlayer = player
    }
    }

        ControlCentreExtras {
            id: extrasView
            objectName: "controlExtrasPage"
            services: root.services
            page: root.detailPage
            x: (1 - root.detailProgress) * 12
            opacity: root.detailProgress
            width: parent.width
            height: parent.height
            visible: root.detailProgress > 0 && root.extraPage
            enabled: root.detailOpen && root.extraPage
            onBackRequested: root.closeDetail()
            onSettingsRequested: panel => root.settings(panel)
        }
        ControlCentreDetails {
            id: detailView
            objectName: "controlDetailPage"
            x: (1 - root.detailProgress) * 12
            opacity: root.detailProgress
            width: parent.width
            height: parent.height
            visible: root.detailProgress > 0 && !root.extraPage
            enabled: root.detailOpen && !root.extraPage
            indicators: root.indicators
            bluetoothAdapter: root.bluetoothAdapter
            page: root.detailPage
            active: root.visible && root.detailOpen
            onBackRequested: root.closeDetail()
            onSettingsRequested: panel => root.settings(panel)
        }
    }

}
