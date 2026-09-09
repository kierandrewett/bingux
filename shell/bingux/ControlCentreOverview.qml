import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

// The live control centre and palette share this view; only their data differs.
GridLayout {
    id: root
    required property var indicators
    required property var services
    property var desktop: DesktopEditing.desktop
    property var widgetLayout: null
    property var barWindow: null
    property bool editingEnabled: true
    property bool active: false
    property var bluetoothAdapter: null
    property var mediaPlayers: []
    property var mediaPlayer: null
    property url accountImageSource: "file:///var/lib/AccountsService/icons/" + Quickshell.env("USER")
    signal settingsRequested(string panel)
    signal detailRequested(string page, var trigger, string audioTab)
    signal lockRequested()
    signal widgetEditRequested(string widgetId, var control)
    signal customiseRequested()
    signal playerSelected(var player)
    function settings(panel) { settingsRequested(panel); }
    function openDetail(page, trigger, audioTab = "") { detailRequested(page, trigger, audioTab); }
    readonly property var groupedLayout: desktop.controlLayout || ControlLayout.defaults()
    function sectionRow(id) { return ControlLayout.position(groupedLayout, "control-centre", id); }
    function groupPosition(group, id) { return ControlLayout.position(groupedLayout, group, id); }
    readonly property var connectedVpns: services.vpns.filter(vpn => vpn.connected)
    readonly property bool showVpn: services.showControl("vpn") && services.vpns.length > 0
    readonly property var microphone: indicators.audioSource || null
    readonly property bool microphoneAvailable: microphone !== null && microphone.ready && microphone.audio !== null
    property var actionWidgets: []
    readonly property var movableWidgets: [networkWidget, bluetoothWidget, vpnWidget, dndWidget, nightLightWidget, powerWidget, awakeWidget, outputControl, inputControl, batteryControl, mediaControl, headerControls, audioRows, quickRows, headerSpace, dividerControl, customiseControl].concat(actionWidgets)
    readonly property alias headerGroup: headerControls
    readonly property alias audioGroup: audioRows
    readonly property alias tileGroup: quickRows
    columns: 1
    rowSpacing: 12
    columnSpacing: 0
    component NativeGroup: GridLayout {
        id: group
        required property string widgetId
        property int nativeRows: -1
        property int nativeColumns: -1
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        readonly property bool placed: !!root.widgetLayout && container !== ""
        readonly property bool barLayout: placed && container !== "sidebar"
        readonly property var barWindow: placed ? root.widgetLayout.windowFor(group) : root.barWindow
        readonly property var memberEntries: root.movableWidgets.filter(item => item.nativeGroup === widgetId && !item.placed).map(item => ({id: item.widgetId, item}))
        readonly property int sidebarColumns: {
            if (container !== "sidebar" || nativeRows !== 1 || !parent) return 0;
            const widths = memberEntries.filter(entry => entry.item.visible).map(entry => entry.item.implicitWidth);
            const total = widths.reduce((sum, width) => sum + width, 0) + Math.max(0, widths.length - 1) * columnSpacing;
            if (total <= parent.width) return 0;
            return Math.max(1, Math.floor((parent.width + columnSpacing) / (Math.max(32, ...widths) + columnSpacing)));
        }
        function memberIndex(id) {
            const order = ControlLayout.items(root.groupedLayout, widgetId);
            return sidebarColumns ? order.filter(value => memberEntries.some(entry => entry.id === value && entry.item.visible)).indexOf(id) : order.indexOf(id);
        }
        function memberRow(id) { return sidebarColumns ? Math.floor(memberIndex(id) / sidebarColumns) : 0; }
        function memberColumn(id) { return sidebarColumns ? memberIndex(id) % sidebarColumns : memberIndex(id); }
        parent: placed ? root.widgetLayout.hostFor(group) : root
        visible: placed || root.sectionRow(widgetId) >= 0
        Layout.fillWidth: !barLayout
        Layout.minimumWidth: DesktopEditing.active ? Theme.barHeight : 0
        Layout.minimumHeight: DesktopEditing.active ? Theme.barHeight : 0
        Layout.row: placed ? root.widgetLayout.controlRow(group) : root.sectionRow(widgetId)
        Layout.column: placed ? root.widgetLayout.controlColumn(group) : 0
        rows: barLayout ? 1 : sidebarColumns ? -1 : nativeRows
        columns: sidebarColumns || (barLayout ? -1 : nativeColumns)
        Component.onCompleted: if (root.editingEnabled) DesktopEditing.registerSource(widgetId, group)
        Component.onDestruction: DesktopEditing.unregisterSource(widgetId, group)
        TapHandler {
            id: groupEdit
            enabled: root.editingEnabled
            acceptedButtons: Qt.RightButton
            acceptedModifiers: Qt.ShiftModifier
            onTapped: {
                if (group.memberEntries.some(entry => entry.item.visible && entry.item.contains(entry.item.mapFromItem(group, groupEdit.point.position)))) return;
                root.widgetEditRequested(group.widgetId, group);
            }
        }
    }
    component ActionWidget: IconButton {
        id: action
        required property string widgetId
        readonly property string nativeGroup: "controls-header"
        readonly property bool placed: container !== ""
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        readonly property bool barLayout: root.widgetLayout !== null && (placed ? container !== "sidebar" : headerControls.barLayout)
        parent: placed ? root.widgetLayout.hostFor(action) : headerControls
        visible: placed || root.groupPosition("controls-header", widgetId) >= 0
        Layout.column: placed ? root.widgetLayout.controlColumn(action) : headerControls.memberColumn(widgetId)
        Layout.row: placed ? root.widgetLayout.controlRow(action) : headerControls.memberRow(widgetId)
        barStyle: barLayout
        barWindow: placed && root.widgetLayout ? root.widgetLayout.windowFor(action) : headerControls.barWindow
        implicitHeight: barLayout ? Theme.barHeight : 32
        presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "controls-header", label, iconName, true, false, placed ? "" : headerControls.container || "control-centre")
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: action; widgetId: action.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        Component.onCompleted: root.actionWidgets = root.actionWidgets.concat([action])
        Component.onDestruction: root.actionWidgets = root.actionWidgets.filter(item => item !== action)
    }
    component AudioWidget: AudioLevel {
        id: audio
        barWindow: placed && root.widgetLayout ? root.widgetLayout.windowFor(audio) : audioRows.barWindow
        required property string widgetId
        readonly property string nativeGroup: "controls-audio"
        readonly property bool placed: container !== ""
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        barLayout: root.widgetLayout !== null && (placed ? container !== "sidebar" : audioRows.barLayout)
        parent: placed ? root.widgetLayout.hostFor(audio) : audioRows
        visible: placed || root.groupPosition("controls-audio", widgetId) >= 0
        Layout.fillWidth: !barLayout
        Layout.preferredWidth: barLayout ? implicitWidth : -1
        Layout.row: placed ? root.widgetLayout.controlRow(audio) : (barLayout ? 0 : root.groupPosition("controls-audio", widgetId))
        Layout.column: placed ? root.widgetLayout.controlColumn(audio) : (barLayout ? root.groupPosition("controls-audio", widgetId) : 0)
        presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "controls-audio", label, muteIconName, true, false, placed ? "" : audioRows.container || "control-centre")
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: audio; widgetId: audio.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
    }
    component QuickWidget: ControlRow {
        id: quick
        readonly property var barWindow: placed && root.widgetLayout ? root.widgetLayout.windowFor(quick) : quickRows.barWindow
        required property string controlName
        readonly property string widgetId: "control-" + controlName
        readonly property string nativeGroup: "controls-tiles"
        readonly property bool placed: container !== ""
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        barLayout: root.widgetLayout !== null && (placed ? container !== "sidebar" : quickRows.barLayout)
        parent: placed ? root.widgetLayout.hostFor(quick) : quickRows
        visible: placed || root.controlVisible(controlName)
        Layout.fillWidth: !barLayout
        Layout.row: placed ? root.widgetLayout.controlRow(quick) : (barLayout ? 0 : root.controlCell(controlName).row)
        Layout.column: placed ? root.widgetLayout.controlColumn(quick) : (barLayout ? root.widgetOrder.indexOf(controlName) : root.controlCell(controlName).column)
        Layout.columnSpan: placed || barLayout ? 1 : root.controlSpan(controlName)
        presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "controls-tiles", title, iconName, true, !barLayout, placed ? "" : quickRows.container || "control-centre")
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: quick; widgetId: quick.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        BarTooltip { anchorItem: quick; barWindow: placed && root.widgetLayout ? root.widgetLayout.windowFor(quick) : quickRows.barWindow; requested: root.editingEnabled && quick.barLayout && quick.hovered; text: quick.displayedTitle + (quick.subtitle ? "\n" + quick.subtitle : "") }
    }
    readonly property var widgetOrder: root.desktop.controlOrder || DesktopLayout.controlOrder()
    function controlVisible(name) {
        if (DesktopLayout.zone(root.desktop.layout || {}, "control-" + name)) return false;
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
    NativeGroup {
        id: headerControls
        objectName: "controlHeader"
        widgetId: "controls-header"
        nativeRows: 1
        rowSpacing: 0
        columnSpacing: 8
        ActionWidget {
            widgetId: "control-account"
            id: accountControl
            objectName: "controlUserAccount"
            iconName: "avatar-default-symbolic"
            imageSource: root.accountImageSource
            label: "User account"
            onClicked: root.settings("users")
        }
        BarSpace {
            id: headerSpace
            objectName: "controlHeaderSpace"
            readonly property string widgetId: "control-header-space"
            readonly property string nativeGroup: "controls-header"
            readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
            readonly property bool placed: container !== ""
            readonly property bool barLayout: placed ? container !== "sidebar" : headerControls.barLayout
            readonly property var barWindow: placed ? root.widgetLayout.windowFor(headerSpace) : headerControls.barWindow
            readonly property bool fixedWidth: root.desktop.widgetOptions?.[widgetId]?.width !== undefined
            gapSize: root.desktop.widgetOptions?.[widgetId]?.width || 16
            flexible: !barLayout && !fixedWidth
            Layout.minimumWidth: barLayout || fixedWidth ? gapSize : 0
            Layout.maximumWidth: barLayout || fixedWidth ? gapSize : Infinity
            parent: placed ? root.widgetLayout.hostFor(headerSpace) : headerControls
            Layout.row: placed ? root.widgetLayout.controlRow(headerSpace) : headerControls.memberRow(widgetId)
            Layout.column: placed ? root.widgetLayout.controlColumn(headerSpace) : headerControls.memberColumn(widgetId)
            visible: placed || root.groupPosition(nativeGroup, widgetId) >= 0
            implicitWidth: barLayout || fixedWidth ? gapSize : 0
            implicitHeight: Theme.barHeight
            Layout.fillWidth: !barLayout && !fixedWidth
            Layout.preferredWidth: barLayout || fixedWidth ? gapSize : -1
            WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: headerSpace; widgetId: headerSpace.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        }
        BatteryStatus {
            id: batteryControl
            objectName: "controlBattery"
            readonly property string widgetId: "control-battery"
            readonly property string nativeGroup: "controls-header"
            readonly property bool placed: container !== ""
            readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
            barLayout: root.widgetLayout !== null && (placed ? container !== "sidebar" : headerControls.barLayout)
            barWindow: placed && root.widgetLayout ? root.widgetLayout.windowFor(batteryControl) : headerControls.barWindow
            parent: placed ? root.widgetLayout.hostFor(batteryControl) : headerControls
            Layout.row: placed ? root.widgetLayout.controlRow(batteryControl) : headerControls.memberRow(widgetId)
            Layout.column: placed ? root.widgetLayout.controlColumn(batteryControl) : headerControls.memberColumn(widgetId)
            visible: (placed || root.groupPosition("controls-header", widgetId) >= 0) && (available || DesktopEditing.active)
            available: root.indicators.laptopBatteryAvailable
            summary: root.indicators.batteryAccessibleName()
            presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "controls-header", label, "battery-good-symbolic", true, true, placed ? "" : headerControls.container || "control-centre")
            WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: batteryControl; widgetId: batteryControl.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        }
        ActionWidget {
            widgetId: "control-settings"
            id: settingsControl
            objectName: "controlSettings"
            iconName: "org.gnome.Settings-symbolic"
            label: "Settings"
            onClicked: root.settings("")
        }
        ActionWidget {
            widgetId: "control-lock"
            id: lockControl
            objectName: "controlLock"
            iconName: "system-lock-screen-symbolic"
            label: "Lock"
            onClicked: root.lockRequested()
        }
    }
    NativeGroup {
        id: audioRows
        objectName: "controlAudioRows"
        widgetId: "controls-audio"
        nativeColumns: 1
        rowSpacing: 12
        columnSpacing: barLayout ? Theme.gap : 0
        AudioWidget {
            widgetId: "control-volume"
            id: outputControl
            objectName: "controlOutputRow"
            node: root.indicators.audioSink || null
            label: "Volume"
            iconName: root.indicators.audioIconName()
            maximum: 1.5
            navigation: true
            navigationObjectName: "controlSoundDetails"
            muteObjectName: "controlMute"
            sliderObjectName: "controlVolume"
            onDevicesRequested: trigger => root.openDetail("audio", outputControl.barLayout ? outputControl : trigger, "output")
        }
        AudioWidget {
            widgetId: "control-microphone"
            id: inputControl
            objectName: "controlInputRow"
            node: root.microphone
            label: "Microphone"
            iconName: "audio-input-microphone-symbolic"
            navigation: true
            navigationObjectName: "controlInputDetails"
            muteObjectName: "controlMicrophoneQuick"
            sliderObjectName: "controlMicrophoneVolume"
            onDevicesRequested: trigger => root.openDetail("audio", inputControl.barLayout ? inputControl : trigger, "input")
        }
    }
    Rectangle {
        id: dividerControl
        objectName: "controlDivider"
        readonly property string widgetId: "control-divider"
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        readonly property bool placed: container !== ""
        readonly property bool barLayout: placed && container !== "sidebar"
        readonly property var barWindow: placed ? root.widgetLayout.windowFor(dividerControl) : root.barWindow
        parent: placed ? root.widgetLayout.hostFor(dividerControl) : root
        Layout.row: placed ? root.widgetLayout.controlRow(dividerControl) : root.sectionRow(widgetId)
        Layout.column: placed ? root.widgetLayout.controlColumn(dividerControl) : 0
        visible: placed || root.sectionRow(widgetId) >= 0
        Layout.fillWidth: !barLayout
        Layout.alignment: Qt.AlignVCenter
        implicitWidth: barLayout ? 1 : 0
        implicitHeight: barLayout ? Theme.barHeight - 12 : 1
        color: Theme.outline; opacity: 0.5
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: dividerControl; anchors.margins: -5; widgetId: dividerControl.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
    }
    NativeGroup {
        id: quickRows
        objectName: "controlQuickRows"
        widgetId: "controls-tiles"
        nativeColumns: 2
        uniformCellWidths: !barLayout
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
        editWidgetId: widgetId
        onEditRequested: (id, item) => root.widgetEditRequested(id, item)
        readonly property string widgetId: "control-media"
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        readonly property bool placed: root.widgetLayout !== null && container !== ""
        barLayout: placed && container !== "sidebar"
        barWindow: root.widgetLayout ? root.widgetLayout.windowFor(mediaControl) : root.barWindow
        parent: placed ? root.widgetLayout.hostFor(mediaControl) : root
        Layout.row: placed ? root.widgetLayout.controlRow(mediaControl) : root.sectionRow(widgetId)
        Layout.column: placed ? root.widgetLayout.controlColumn(mediaControl) : 0
        visible: placed || root.sectionRow(widgetId) >= 0
        Layout.fillWidth: !barLayout
        presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "control-centre", player ? player.trackTitle || player.identity : "Media playback", "applications-multimedia-symbolic", true, true)
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: mediaControl; widgetId: mediaControl.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        player: root.mediaPlayer
        playerOptions: root.mediaPlayers
        onPlayerSelected: selectedPlayer => root.playerSelected(selectedPlayer)
        active: root.active || placed
    }


    ActionButton {
        id: customiseControl
        objectName: "controlCustomise"
        readonly property string widgetId: "control-customise"
        readonly property string container: DesktopLayout.zone(root.desktop.layout || {}, widgetId)
        readonly property bool placed: container !== ""
        readonly property bool barLayout: placed && container !== "sidebar"
        readonly property var barWindow: placed ? root.widgetLayout.windowFor(customiseControl) : root.barWindow
        parent: placed ? root.widgetLayout.hostFor(customiseControl) : root
        Layout.row: placed ? root.widgetLayout.controlRow(customiseControl) : root.sectionRow(widgetId)
        Layout.column: placed ? root.widgetLayout.controlColumn(customiseControl) : 0
        visible: placed || root.sectionRow(widgetId) >= 0
        Layout.alignment: Qt.AlignRight
        implicitHeight: barLayout ? Theme.barHeight : 28
        flat: true
        text: "Customise controls..."
        iconName: "document-edit-symbolic"
        presentation: DesktopLayout.presentation(root.desktop, widgetId, container || "control-centre", text, iconName, barLayout, !barLayout)
        WidgetEditHandle { previewSource: root.editingEnabled; visible: root.editingEnabled; control: customiseControl; widgetId: customiseControl.widgetId; onRequested: (id, item) => root.widgetEditRequested(id, item) }
        BarTooltip { anchorItem: customiseControl; barWindow: customiseControl.barWindow; requested: root.editingEnabled && customiseControl.barLayout && customiseControl.hovered; text: customiseControl.displayedText }
        onClicked: root.customiseRequested()
    }
    Text { Layout.row: ControlLayout.items(root.groupedLayout, "control-centre").length; Layout.fillWidth: true; visible: root.services.error !== ""; text: root.services.error; wrapMode: Text.Wrap; color: Theme.muted; font.pixelSize: Theme.fontSmall }
}
