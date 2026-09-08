import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Services.Mpris
import "DesktopLayout.js" as DesktopLayout

ShellPopup {
    id: root
    keepWindowAlive: DesktopEditing.editor !== null
    required property var indicators
    signal widgetEditRequested(string widgetId, var control)
    signal customiseRequested()
    readonly property var widgetOrder: DesktopEditing.desktop.controlOrder || DesktopLayout.controlOrder()
    function controlVisible(name) {
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
    onVisibleChanged: services.active = visible
    property bool detailOpen: false
    property string detailPage: "network"
    property real detailProgress: detailOpen ? 1 : 0
    Behavior on detailProgress { NumberAnimation { duration: Theme.reducedMotion ? 0 : 160; easing.type: Easing.OutCubic } }
    property var detailTrigger: null
    property bool pendingDetailFocus: false
    property bool pendingOverviewFocus: false
    function openDetail(page, trigger, audioTab) {
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
    Connections { target: root; function onVisibleChanged() { if (!root.visible) root.detailOpen = false; } }
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
    readonly property real maximumPopupHeight: Math.max(0, Math.min(height * 0.8, anchorAbove ? anchorTop - Theme.barHeight - Theme.gap : dockSafeBottom - belowAnchorY))
    property real controlsHeight: Math.min(detailOpen ? activeDetailView.implicitHeight : controls.implicitHeight,
        Math.max(0, maximumPopupHeight - contentPadding * 2))
    Behavior on controlsHeight {
        enabled: root.visible && root.revealScale === 1
        NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
    }
    popupHeight: Math.ceil(controlsHeight) + contentPadding * 2
    contentPadding: 16
    cornerRadius: Theme.cardRadius
    surfaceColor: Theme.shellSurface
    preferredX: DesktopEditing.active ? width - popupWidth - DesktopEditing.editor.rightInset - 24 : anchorItem ? anchorPosition.x - popupWidth : width - popupWidth - Theme.padding

    preferredY: DesktopEditing.active ? Theme.barHeight + 40 + DesktopEditing.editor.topInset : anchorItem ? (anchorAbove ? anchorTop - popupHeight - Theme.gap : anchorPosition.y + Theme.gap) : Theme.barHeight + Theme.gap
    dismissOnOutsideClick: !DesktopEditing.active
    keyboardInteractive: !DesktopEditing.active
    NativeEditSurface {
        parent: root.body.parent; anchors.fill: parent; window: root.nativeWindow; zoneName: "control-centre"; vertical: true
        entries: [{id: "control-network", item: networkWidget}, {id: "control-bluetooth", item: bluetoothWidget}, {id: "control-vpn", item: vpnWidget}, {id: "control-dnd", item: dndWidget}, {id: "control-nightLight", item: nightLightWidget}, {id: "control-power", item: powerWidget}, {id: "control-awake", item: awakeWidget}]
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
    ColumnLayout {
        id: controls
        objectName: "controlOverviewRows"
        width: overview.width
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            IconButton {
                objectName: "controlUserAccount"
                iconName: "avatar-default-symbolic"
                imageSource: "file:///var/lib/AccountsService/icons/" + Quickshell.env("USER")
                label: "User account"
                onClicked: root.settings("users")
            }
            Item { Layout.fillWidth: true }
            SymbolicIcon { visible: root.indicators.laptopBatteryAvailable; implicitSize: 16; color: Theme.muted; source: Quickshell.iconPath("battery-good-symbolic") }
            Text { visible: root.indicators.laptopBatteryAvailable; text: root.indicators.batteryAccessibleName().replace(/^Battery /, "").replace(" percent", "%").replace(/,.*$/, ""); Accessible.name: root.indicators.batteryAccessibleName(); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
            IconButton {
                objectName: "controlSettings"
                iconName: "org.gnome.Settings-symbolic"
                label: "Settings"
                onClicked: root.settings("")
            }
            IconButton {
                objectName: "controlLock"
                iconName: "system-lock-screen-symbolic"
                label: "Lock"
                onClicked: { Quickshell.execDetached(["loginctl", "lock-session"]); root.visible = false }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 12
            AudioLevel {
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
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.outline; opacity: 0.5 }
        GridLayout {
            id: quickRows
            Layout.fillWidth: true
            columns: 2
            uniformCellWidths: true
            rowSpacing: 8
            columnSpacing: 8
            ControlRow {
                tileLayout: true
                Layout.columnSpan: 1
                navigation: true
                rowInteractive: false
                id: networkWidget
                objectName: "controlNetwork"
                WidgetEditHandle { control: networkWidget; widgetId: "control-network"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-network", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("network")
                Layout.row: root.controlCell("network").row
                Layout.column: root.controlCell("network").column
                iconName: root.indicators.networkIconName()
                title: root.indicators.networkState === "wired" ? "Ethernet" : root.indicators.networkState === "vpn" ? "VPN" : "Wi-Fi"
                subtitle: root.indicators.networkState === "offline" ? "Not connected" : root.indicators.networkState === "unknown" ? "Unavailable" : "Connected"
                selected: root.indicators.networkState !== "offline" && root.indicators.networkState !== "unknown"
                Accessible.description: root.indicators.networkAccessibleName()
                onNavigationRequested: trigger => root.openDetail("network", trigger)
            }
            ControlRow {
                tileLayout: true
                Layout.columnSpan: 1
                navigation: true
                rowInteractive: false
                id: bluetoothWidget
                objectName: "controlBluetooth"
                WidgetEditHandle { control: bluetoothWidget; widgetId: "control-bluetooth"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-bluetooth", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("bluetooth")
                Layout.row: root.controlCell("bluetooth").row
                Layout.column: root.controlCell("bluetooth").column
                iconName: "bluetooth-active-symbolic"
                title: "Bluetooth"
                toggleVisible: true
                enabled: root.bluetoothAdapter !== null
                selected: root.bluetoothAdapter !== null && root.bluetoothAdapter.enabled
                subtitle: root.bluetoothAdapter ? (root.bluetoothAdapter.enabled ? "On" : "Off") : "Unavailable"
                onNavigationRequested: trigger => root.openDetail("bluetooth", trigger)
                onToggleRequested: if (root.bluetoothAdapter) root.bluetoothAdapter.enabled = !root.bluetoothAdapter.enabled
            }
            ControlRow {
                visible: root.controlVisible("vpn")
                Layout.row: root.controlCell("vpn").row
                Layout.column: root.controlCell("vpn").column
                tileLayout: false
                tileSurface: true
                Layout.columnSpan: 2
                id: vpnWidget
                objectName: "controlVpn"
                WidgetEditHandle { control: vpnWidget; widgetId: "control-vpn"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-vpn", "control-centre", title, iconName, true, true)
                rowInteractive: false
                navigation: true
                iconName: "network-vpn-symbolic"
                title: "VPN"
                selected: root.connectedVpns.length > 0
                subtitle: root.connectedVpns.length ? root.connectedVpns.map(vpn => vpn.name).join(", ") : "Disconnected"
                onNavigationRequested: trigger => root.openDetail("vpn", trigger)
            }
            ControlRow {
                tileLayout: true
                compactTile: true
                Layout.columnSpan: root.controlSpan("dnd")
                Layout.row: root.controlCell("dnd").row
                Layout.column: root.controlCell("dnd").column
                id: dndWidget
                objectName: "controlDnd"
                WidgetEditHandle { control: dndWidget; widgetId: "control-dnd"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-dnd", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("dnd")
                title: "Do Not Disturb"
                subtitle: ""
                iconName: "notifications-disabled-symbolic"
                rowInteractive: false
                toggleVisible: true
                toggleEnabled: !!root.services.state.dndAvailable && !root.services.busy
                selected: root.services.doNotDisturb
                onToggleRequested: root.services.action({kind: "dnd", enabled: !root.services.doNotDisturb})
            }
            ControlRow {
                tileLayout: true
                compactTile: true
                Layout.columnSpan: root.controlSpan("nightLight")
                Layout.row: root.controlCell("nightLight").row
                Layout.column: root.controlCell("nightLight").column
                id: nightLightWidget
                objectName: "controlNightLight"
                WidgetEditHandle { control: nightLightWidget; widgetId: "control-nightLight"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-nightLight", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("nightLight")
                title: "Night Light"
                subtitle: !root.services.state.nightLightAvailable ? "Unavailable" : root.services.state.nightLight && !root.services.state.nightLightActive ? "Scheduled" : ""
                iconName: "night-light-symbolic"
                rowInteractive: false
                toggleVisible: true
                toggleEnabled: !!root.services.state.nightLightAvailable && !root.services.busy
                selected: !!root.services.state.nightLight
                onToggleRequested: root.services.action({kind: "nightLight", enabled: !root.services.state.nightLight})
            }
            ControlRow {
                tileLayout: false
                tileSurface: true
                Layout.columnSpan: 2
                id: powerWidget
                objectName: "controlPower"
                WidgetEditHandle { control: powerWidget; widgetId: "control-power"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-power", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("power")
                Layout.row: root.controlCell("power").row
                Layout.column: root.controlCell("power").column
                title: "Power mode"
                subtitle: !root.services.state.power.available ? "Unavailable" : root.services.state.power.profile === "power-saver" ? "Power Saver" : root.services.state.power.profile === "performance" ? "Performance" : "Balanced"
                iconName: "power-profile-balanced-symbolic"
                rowInteractive: false
                navigation: true
                enabled: root.services.state.power.available
                onNavigationRequested: trigger => root.openDetail("power", trigger)
            }
            ControlRow {
                tileLayout: false
                tileSurface: true
                Layout.columnSpan: 2
                id: awakeWidget
                objectName: "controlKeepAwake"
                WidgetEditHandle { control: awakeWidget; widgetId: "control-awake"; onRequested: (id, item) => root.widgetEditRequested(id, item) }
                presentation: DesktopLayout.presentation(DesktopEditing.desktop, "control-awake", "control-centre", title, iconName, true, true)
                visible: root.controlVisible("awake")
                Layout.row: root.controlCell("awake").row
                Layout.column: root.controlCell("awake").column
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
            Layout.fillWidth: true
            player: root.mediaPlayer
            playerOptions: root.mediaPlayers
            onPlayerSelected: selectedPlayer => root.selectedMediaPlayer = selectedPlayer
            active: root.visible
        }


        ActionButton {
            objectName: "controlCustomise"
            Layout.alignment: Qt.AlignRight
            implicitHeight: 28
            flat: true
            text: "Customise controls..."
            onClicked: root.customiseRequested()
        }
        Text { Layout.fillWidth: true; visible: root.services.error !== ""; text: root.services.error; wrapMode: Text.Wrap; color: Theme.muted; font.pixelSize: Theme.fontSmall }
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
