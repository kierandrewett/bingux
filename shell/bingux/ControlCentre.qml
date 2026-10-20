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
    keepWindowAlive: false
    readonly property var groupedLayout: controls.groupedLayout
    function snapshotLayout() {
        return JSON.parse(JSON.stringify(BinguxPreferences.data.desktop.controlLayout || ControlLayout.defaults()));
    }
    function sectionRow(id) {
        return ControlLayout.position(groupedLayout, "control-centre", id);
    }
    function groupPosition(group, id) {
        return ControlLayout.position(groupedLayout, group, id);
    }
    required property var indicators
    property var widgetLayout: null
    readonly property alias widgetHost: controls
    readonly property var externalEntries: widgetLayout ? widgetLayout.defaultControls.map(item => ({
                id: widgetLayout.nameFor(item),
                item
            })).filter(entry => ControlLayout.isExternal(entry.id) && ControlLayout.contains(groupedLayout, "control-centre", entry.id)) : []
    property Item movedAnchor: null
    readonly property var actionWidgets: controls.actionWidgets
    readonly property var movableWidgets: controls.movableWidgets
    readonly property var widgetOrder: controls.widgetOrder
    function controlVisible(name) {
        return controls.controlVisible(name);
    }
    function controlSpan(name) {
        return controls.controlSpan(name);
    }
    function controlCell(name) {
        return controls.controlCell(name);
    }
    // The detail pages are built on first use so they stay out of shell start-up.
    property bool pagesRequested: false
    function extrasPage() {
        pagesRequested = true;
        return extrasLoader.item;
    }
    function detailPageItem() {
        pagesRequested = true;
        return detailLoader.item;
    }
    function deviceControls() {
        return detailPageItem();
    }
    property var services: ControlCentreServices
    readonly property bool extraPage: ["vpn", "power", "customise", "session"].includes(detailPage)
    readonly property var activeDetailView: extraPage ? extrasLoader.item : detailLoader.item
    readonly property var connectedVpns: services.vpns.filter(vpn => vpn.connected)
    readonly property bool showVpn: services.showControl("vpn") && services.vpns.length > 0
    Binding {
        target: root.services
        property: "active"
        value: root.visible || root.movableWidgets.some(item => item.placed)
    }
    property bool detailOpen: false
    property string detailPage: "network"
    property real detailProgress: detailOpen ? 1 : 0
    Behavior on detailProgress {
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 160
            easing.type: Easing.OutCubic
        }
    }
    property var detailTrigger: null
    property bool pendingDetailFocus: false
    property bool pendingOverviewFocus: false
    function openDetail(page, trigger, audioTab) {
        let widget = trigger;
        while (widget && !widget.widgetId)
            widget = widget.parent;
        if (widget && widget.Window.window !== root.body.Window.window) {
            movedAnchor = widget;
            visible = true;
        }
        pagesRequested = true;
        if (page === "audio" && detailLoader.item)
            detailLoader.item.audioTab = audioTab || "output";
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
        onTriggered: if (root.activeDetailView && root.activeDetailView.visible) {
            root.activeDetailView.focusBack(Qt.TabFocusReason);
            root.pendingDetailFocus = false;
        }
    }
    function closeDetail() {
        const keyboard = activeDetailView?.keyboardNavigation;
        detailOpen = false;
        pendingOverviewFocus = keyboard && !!detailTrigger;
    }
    Timer {
        interval: 16
        repeat: true
        running: root.pendingOverviewFocus && !root.detailOpen
        onTriggered: {
            if (!root.visible)
                root.pendingOverviewFocus = false;
            else if (controls.visible) {
                if (root.detailTrigger)
                    root.detailTrigger.forceActiveFocus(Qt.TabFocusReason);
                root.pendingOverviewFocus = false;
            }
        }
    }
    Connections {
        target: root
        function onVisibleChanged() {
            if (!root.visible) {
                root.detailOpen = false;
                root.movedAnchor = null;
            }
        }
    }
    property var bluetoothAdapter: Bluetooth.defaultAdapter
    readonly property var microphone: indicators.audioSource || null
    readonly property bool microphoneAvailable: microphone !== null && microphone.ready && microphone.audio !== null
    property var mediaPlayers: Mpris.players.values
    property var selectedMediaPlayer: null
    property var mediaPlayer: mediaPlayers.indexOf(selectedMediaPlayer) >= 0 ? selectedMediaPlayer : mediaPlayers.find(player => player.isPlaying) || mediaPlayers[0] || null
    popupWidth: Theme.notificationWidth + contentPadding * 2
    property real dockSafeInset: Theme.dockExclusiveHeight
    popupDockSafeBottom: height - dockSafeInset - Theme.gap
    property real controlsHeight: Math.min(detailOpen ? (activeDetailView?.implicitHeight || 0) : Math.max(96, controls.implicitHeight), Math.max(0, maximumPopupHeight - contentPadding * 2))
    Behavior on controlsHeight {
        enabled: root.visible && root.revealScale === 1
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
        }
    }
    popupHeight: Math.ceil(controlsHeight) + contentPadding * 2
    preferredX: anchorItem ? anchorPosition.x - popupWidth : width - popupWidth - Theme.padding
    preferredY: anchorItem ? (anchorAbove ? anchorTop - popupHeight - Theme.gap : anchorPosition.y + Theme.gap) : Theme.barHeight + Theme.gap
    dismissOnOutsideClick: true
    keyboardInteractive: true

    function settings(panel) {
        Quickshell.execDetached([Python.python, Python.applicationSpawn, "--", Python.settings].concat(panel ? [panel] : []));
        visible = false;
    }

    Item {
        id: controlDeck
        objectName: "controlLiveWidgets"
        enabled: true
        opacity: 1
        width: parent.width
        height: root.controlsHeight
        Text {
            anchors.centerIn: parent
            visible: !root.detailOpen && controls.implicitHeight === 0
            text: "No controls here"
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
            transform: Translate {
                x: -root.detailProgress * 12
            }
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
                onLockRequested: {
                    Quickshell.execDetached(["bingux-lock"]);
                    root.visible = false;
                }
                onCustomiseRequested: root.openDetail("customise", null, "")
                onPlayerSelected: player => root.selectedMediaPlayer = player
            }
        }

        Loader {
            id: extrasLoader
            active: root.pagesRequested || root.detailOpen
            x: (1 - root.detailProgress) * 12
            width: parent.width
            height: parent.height
            sourceComponent: ControlCentreExtras {
                objectName: "controlExtrasPage"
                services: root.services
                page: root.detailPage
                opacity: root.detailProgress
                anchors.fill: parent
                visible: root.detailProgress > 0 && root.extraPage
                enabled: root.detailOpen && root.extraPage
                onBackRequested: root.closeDetail()
                onSettingsRequested: panel => root.settings(panel)
            }
        }
        Loader {
            id: detailLoader
            active: root.pagesRequested || root.detailOpen
            x: (1 - root.detailProgress) * 12
            width: parent.width
            height: parent.height
            sourceComponent: ControlCentreDetails {
                objectName: "controlDetailPage"
                opacity: root.detailProgress
                anchors.fill: parent
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
}
