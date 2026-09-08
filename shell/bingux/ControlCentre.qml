import QtQuick
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
    readonly property var groupedLayout: DesktopEditing.desktop.controlLayout || ControlLayout.defaults()
    function snapshotLayout() { return JSON.parse(JSON.stringify(BinguxPreferences.data.desktop.controlLayout || ControlLayout.defaults())); }
    function sectionRow(id) { return ControlLayout.position(groupedLayout, "control-centre", id); }
    function groupPosition(group, id) { return ControlLayout.position(groupedLayout, group, id); }
    required property var indicators
    signal widgetEditRequested(string widgetId, var control)
    signal customiseRequested()
    property var widgetLayout: null
    property Item movedAnchor: null
    readonly property var groupedEntries: [
        {id: "control-account", item: accountControl}, {id: "control-header-space", item: headerSpace},
        {id: "control-battery", item: batteryControl}, {id: "control-settings", item: settingsControl},
        {id: "control-session", item: headerControls.children.find(item => item.objectName === "controlSessionPower")},
        {id: "control-lock", item: lockControl}, {id: "control-volume", item: outputControl},
        {id: "control-microphone", item: inputControl}, {id: "control-media", item: mediaControl},
        {id: "control-divider", item: dividerControl}, {id: "control-customise", item: customiseControl},
        {id: "controls-header", item: headerControls}, {id: "controls-audio", item: audioRows},
        {id: "controls-tiles", item: quickRows}
    ]
    readonly property var movableWidgets: [networkWidget, bluetoothWidget, vpnWidget, dndWidget, nightLightWidget, powerWidget, awakeWidget]
    component QuickWidget: ControlRow {
        id: quick
        required property string controlName
        readonly property string widgetId: "control-" + controlName
        readonly property string container: DesktopLayout.zone(DesktopEditing.desktop.layout || {}, widgetId)
        barLayout: root.widgetLayout !== null && container !== ""
        parent: barLayout ? root.widgetLayout.hostFor(quick) : quickRows
        visible: barLayout || root.controlVisible(controlName)
        Layout.fillWidth: !barLayout
        Layout.row: barLayout ? root.widgetLayout.controlRow(quick) : root.controlCell(controlName).row
        Layout.column: barLayout ? root.widgetLayout.controlColumn(quick) : root.controlCell(controlName).column
        Layout.columnSpan: barLayout ? 1 : root.controlSpan(controlName)
        presentation: DesktopLayout.presentation(DesktopEditing.desktop, widgetId, container || "control-centre", title, iconName, true, !barLayout)
        WidgetEditHandle { control: quick; widgetId: quick.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        BarTooltip { anchorItem: quick; barWindow: root.widgetLayout ? root.widgetLayout.windowFor(quick) : root.nativeWindow; requested: quick.barLayout && quick.hovered; text: quick.displayedTitle + (quick.subtitle ? "\n" + quick.subtitle : "") }
    }
    readonly property var widgetOrder: DesktopEditing.desktop.controlOrder || DesktopLayout.controlOrder()
    function controlVisible(name) {
        if (DesktopLayout.zone(DesktopEditing.desktop.layout || {}, "control-" + name)) return false;
        if (name === "awake" && services.keepAwake) return true;
        if (!widgetOrder.includes(name)) return false;
        return name === "network" || name === "bluetooth" ? true : name === "vpn" ? showVpn : services.showControl(name);
    }
    function controlSpan(name) {
        if (["vpn", "power", "awake"].includes(name)) return 2;
        if (["dnd", "nightLight"].includes(name)) return controlVisible("dnd") && controlVisible("nightLight") ? 1 : 2;
        return 1;
    }
    function controlCell(name) {
        let row = 0, column = 0;
        const order = widgetOrder.includes("awake") ? widgetOrder : widgetOrder.concat(["awake"]);
        for (const id of order) {
            if (!controlVisible(id)) continue;
            const span = controlSpan(id);
            if (column + span > 2) { row++; column = 0; }
            if (id === name) return {row, column};
            column += span;
            if (column === 2) { row++; column = 0; }
        }
        return {row: -1, column: -1};
    }
    readonly property var controlChoices: extrasView.choices
    readonly property var deviceControls: detailView
    property var services: ControlCentreServices
    readonly property bool extraPage: ["vpn", "power", "customise"].includes(detailPage)
    readonly property var activeDetailView: extraPage ? extrasView : detailView
    readonly property var connectedVpns: services.vpns.filter(vpn => vpn.connected)
    readonly property bool showVpn: services.showControl("vpn") && services.vpns.length > 0
    Binding { target: root.services; property: "active"; value: root.visible || root.movableWidgets.some(item => item.barLayout) }
    property bool detailOpen: false
    property string detailPage: "network"
    property real detailProgress: detailOpen ? 1 : 0
    Behavior on detailProgress { NumberAnimation { duration: Theme.reducedMotion ? 0 : 160; easing.type: Easing.OutCubic } }
    property var detailTrigger: null
    property bool pendingDetailFocus: false
    property bool pendingOverviewFocus: false
    function openDetail(page, trigger, audioTab) {
        if (trigger?.barLayout) { movedAnchor = trigger; visible = true; }
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
    property real controlsHeight: Math.min(detailOpen ? activeDetailView.implicitHeight : controls.implicitHeight,
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
            return group === "controls-header" && headerControls.visible ? headerControls
                : group === "controls-audio" && audioRows.visible ? audioRows : root.body.parent;
        }
        entries: root.movableWidgets.filter(item => !item.barLayout).map(item => ({id: item.widgetId, item})).concat(root.groupedEntries)
    }

    function settings(panel) {
        Quickshell.execDetached(["gnome-control-center", panel]);
        visible = false;
    }

    Item {
        id: controlDeck
        width: parent.width
        height: root.controlsHeight
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
    GridLayout {
        id: controls
        objectName: "controlOverviewRows"
        width: overview.width
        columns: 1
        rowSpacing: 12
        columnSpacing: 0
        GridLayout {
            id: headerControls
            objectName: "controlHeader"
            Layout.row: root.sectionRow("controls-header")
            visible: root.sectionRow("controls-header") >= 0
            Layout.fillWidth: true
            rows: 1
            rowSpacing: 0
            columnSpacing: 8
            IconButton {
                id: accountControl
                objectName: "controlUserAccount"
                Layout.column: root.groupPosition("controls-header", "control-account")
                visible: root.groupPosition("controls-header", "control-account") >= 0
                iconName: "avatar-default-symbolic"
                imageSource: "file:///var/lib/AccountsService/icons/" + Quickshell.env("USER")
                label: "User account"
                onClicked: root.settings("users")
            }
            Item {
                id: headerSpace
                objectName: "controlHeaderSpace"
                Layout.column: root.groupPosition("controls-header", "control-header-space")
                visible: root.groupPosition("controls-header", "control-header-space") >= 0
                Layout.fillWidth: true
            }
            RowLayout {
                id: batteryControl
                objectName: "controlBattery"
                Layout.column: root.groupPosition("controls-header", "control-battery")
                visible: root.groupPosition("controls-header", "control-battery") >= 0 && root.indicators.laptopBatteryAvailable
                spacing: 8
                SymbolicIcon { implicitSize: 16; color: Theme.muted; source: Quickshell.iconPath("battery-good-symbolic") }
                Text { text: root.indicators.batteryAccessibleName().replace(/^Battery /, "").replace(" percent", "%").replace(/,.*$/, ""); Accessible.name: root.indicators.batteryAccessibleName(); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            }
            IconButton {
                id: settingsControl
                objectName: "controlSettings"
                Layout.column: root.groupPosition("controls-header", "control-settings")
                visible: root.groupPosition("controls-header", "control-settings") >= 0
                iconName: "org.gnome.Settings-symbolic"
                label: "Settings"
                onClicked: root.settings("")
            }
            IconButton {
                id: lockControl
                objectName: "controlLock"
                Layout.column: root.groupPosition("controls-header", "control-lock")
                visible: root.groupPosition("controls-header", "control-lock") >= 0
                iconName: "system-lock-screen-symbolic"
                label: "Lock"
                onClicked: { Quickshell.execDetached(["loginctl", "lock-session"]); root.visible = false }
            }
        }
        GridLayout {
            id: audioRows
            objectName: "controlAudioRows"
            Layout.row: root.sectionRow("controls-audio")
            visible: root.sectionRow("controls-audio") >= 0
            Layout.fillWidth: true
            columns: 1
            rowSpacing: 12
            columnSpacing: 0
            AudioLevel {
                id: outputControl
                objectName: "controlOutputRow"
                Layout.row: root.groupPosition("controls-audio", "control-volume")
                visible: root.groupPosition("controls-audio", "control-volume") >= 0
                node: root.indicators.audioSink || null
                label: "Volume"
                iconName: root.indicators.audioIconName()
                maximum: 1.5
                navigation: true
                navigationObjectName: "controlSoundDetails"
                muteObjectName: "controlMute"
                sliderObjectName: "controlVolume"
                onDevicesRequested: trigger => root.openDetail("audio", trigger, "output")
            }
            AudioLevel {
                id: inputControl
                objectName: "controlInputRow"
                Layout.row: root.groupPosition("controls-audio", "control-microphone")
                visible: root.groupPosition("controls-audio", "control-microphone") >= 0
                node: root.microphone
                label: "Microphone"
                iconName: "audio-input-microphone-symbolic"
                navigation: true
                navigationObjectName: "controlInputDetails"
                muteObjectName: "controlMicrophoneQuick"
                sliderObjectName: "controlMicrophoneVolume"
                onDevicesRequested: trigger => root.openDetail("audio", trigger, "input")
            }
        }
        Rectangle {
            id: dividerControl
            objectName: "controlDivider"
            Layout.row: root.sectionRow("control-divider")
            visible: root.sectionRow("control-divider") >= 0
            Layout.fillWidth: true; implicitHeight: 1; color: Theme.outline; opacity: 0.5
        }
        GridLayout {
            id: quickRows
            objectName: "controlQuickRows"
            Layout.row: root.sectionRow("controls-tiles")
            visible: root.sectionRow("controls-tiles") >= 0
            Layout.fillWidth: true
            columns: 2
            uniformCellWidths: true
            rowSpacing: 8
            columnSpacing: 8
            QuickWidget {
                tileLayout: true
                navigation: true
                rowInteractive: false
                id: networkWidget
                controlName: "network"
                objectName: "controlNetwork"
                iconName: root.indicators.networkIconName()
                title: root.indicators.networkState === "wired" ? "Ethernet" : root.indicators.networkState === "vpn" ? "VPN" : "Wi-Fi"
                subtitle: root.indicators.networkState === "offline" ? "Not connected" : root.indicators.networkState === "unknown" ? "Unavailable" : "Connected"
                selected: root.indicators.networkState !== "offline" && root.indicators.networkState !== "unknown"
                Accessible.description: root.indicators.networkAccessibleName()
                onNavigationRequested: trigger => root.openDetail("network", trigger)
            }
            QuickWidget {
                tileLayout: true
                navigation: true
                rowInteractive: false
                id: bluetoothWidget
                controlName: "bluetooth"
                objectName: "controlBluetooth"
                iconName: "bluetooth-active-symbolic"
                title: "Bluetooth"
                toggleVisible: true
                enabled: root.bluetoothAdapter !== null
                selected: root.bluetoothAdapter !== null && root.bluetoothAdapter.enabled
                subtitle: root.bluetoothAdapter ? (root.bluetoothAdapter.enabled ? "On" : "Off") : "Unavailable"
                onNavigationRequested: trigger => root.openDetail("bluetooth", trigger)
                onToggleRequested: if (root.bluetoothAdapter) root.bluetoothAdapter.enabled = !root.bluetoothAdapter.enabled
            }
            QuickWidget {
                tileLayout: false
                tileSurface: true
                id: vpnWidget
                controlName: "vpn"
                objectName: "controlVpn"
                rowInteractive: false
                navigation: true
                iconName: "network-vpn-symbolic"
                title: "VPN"
                selected: root.connectedVpns.length > 0
                subtitle: root.connectedVpns.length ? root.connectedVpns.map(vpn => vpn.name).join(", ") : "Disconnected"
                onNavigationRequested: trigger => root.openDetail("vpn", trigger)
            }
            QuickWidget {
                tileLayout: true
                compactTile: true
                id: dndWidget
                controlName: "dnd"
                objectName: "controlDnd"
                title: "Do Not Disturb"
                subtitle: ""
                iconName: "notifications-disabled-symbolic"
                rowInteractive: false
                toggleVisible: true
                toggleEnabled: !!root.services.state.dndAvailable && !root.services.busy
                selected: root.services.doNotDisturb
                onToggleRequested: root.services.action({kind: "dnd", enabled: !root.services.doNotDisturb})
            }
            QuickWidget {
                tileLayout: true
                compactTile: true
                id: nightLightWidget
                controlName: "nightLight"
                objectName: "controlNightLight"
                title: "Night Light"
                subtitle: !root.services.state.nightLightAvailable ? "Unavailable" : root.services.state.nightLight && !root.services.state.nightLightActive ? "Scheduled" : ""
                iconName: "night-light-symbolic"
                rowInteractive: false
                toggleVisible: true
                toggleEnabled: !!root.services.state.nightLightAvailable && !root.services.busy
                selected: !!root.services.state.nightLight
                onToggleRequested: root.services.action({kind: "nightLight", enabled: !root.services.state.nightLight})
            }
            QuickWidget {
                tileLayout: false
                tileSurface: true
                id: powerWidget
                controlName: "power"
                objectName: "controlPower"
                title: "Power mode"
                subtitle: !root.services.state.power.available ? "Unavailable" : root.services.state.power.profile === "power-saver" ? "Power Saver" : root.services.state.power.profile === "performance" ? "Performance" : "Balanced"
                iconName: "power-profile-balanced-symbolic"
                rowInteractive: false
                navigation: true
                enabled: root.services.state.power.available
                onNavigationRequested: trigger => root.openDetail("power", trigger)
            }
            QuickWidget {
                tileLayout: false
                tileSurface: true
                id: awakeWidget
                controlName: "awake"
                objectName: "controlKeepAwake"
                title: "Keep Awake"
                subtitle: root.services.keepAwake ? "Until sign out" : ""
                iconName: "display-brightness-symbolic"
                rowInteractive: false
                toggleVisible: true
                toggleEnabled: !!root.services.state.awakeAvailable
                selected: root.services.keepAwake
                onToggleRequested: root.services.toggleAwake()
            }
        }
        ControlCentreMedia {
            id: mediaControl
            objectName: "controlMediaCard"
            Layout.row: root.sectionRow("control-media")
            visible: root.sectionRow("control-media") >= 0
            Layout.fillWidth: true
            player: root.mediaPlayer
            playerOptions: root.mediaPlayers
            onPlayerSelected: selectedPlayer => root.selectedMediaPlayer = selectedPlayer
            active: root.visible
        }


        ActionButton {
            id: customiseControl
            objectName: "controlCustomise"
            Layout.row: root.sectionRow("control-customise")
            visible: root.sectionRow("control-customise") >= 0
            Layout.alignment: Qt.AlignRight
            implicitHeight: 28
            flat: true
            text: "Customise controls..."
            onClicked: root.customiseRequested()
        }
        Text { Layout.row: ControlLayout.items(root.groupedLayout, "control-centre").length; Layout.fillWidth: true; visible: root.services.error !== ""; text: root.services.error; wrapMode: Text.Wrap; color: Theme.muted; font.pixelSize: Theme.fontSmall }
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
