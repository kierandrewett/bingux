import QtQuick
import QtQuick.Layouts
import Quickshell
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

// The palette uses the normal visual components with quiet sample data.
FocusScope {
    id: root
    required property string widgetId
    property var metrics: null
    readonly property var spec: ControlLayout.widget(widgetId) || DesktopLayout.widget(widgetId) || {}
    readonly property bool groupedWidget: !!ControlLayout.groupFor(widgetId)
    readonly property bool nativeGroupPreview: ["controls-header", "controls-audio", "controls-tiles"].includes(widgetId)
    readonly property bool utilityWidget: ["control-divider", "control-header-space", "control-customise"].includes(widgetId)
    readonly property bool panelWidget: !!spec.panel
    readonly property string container: DesktopLayout.placement(DesktopEditing.desktop, widgetId) || (groupedWidget || widgetId.startsWith("control-") ? "control-centre" : "top-right")
    readonly property var face: DesktopLayout.presentation(DesktopEditing.desktop, widgetId, container, spec.label || "", spec.icon || "", !["clock", "keyboard"].includes(widgetId), ["clock", "keyboard"].includes(widgetId))
    readonly property Item visualItem: frame
    readonly property Item previewControl: component.item
    clip: true
    function isolateKeyboard(item) {
        item.activeFocusOnTab = false;
        for (const child of item.children) isolateKeyboard(child);
    }
    Item {
        id: frame
        anchors.centerIn: parent
        width: Math.min(root.width, component.width * component.scale)
        height: Math.min(root.height, component.height * component.scale)
        clip: true
        Loader {
            id: component
            active: DesktopEditing.active
            onLoaded: root.isolateKeyboard(item)
            width: root.nativeGroupPreview ? (["control-centre", "sidebar"].includes(root.container) ? Theme.notificationWidth : item ? item.implicitWidth : 0)
                : root.utilityWidget ? (item ? item.implicitWidth : 0) : root.groupedWidget ? (root.spec.group ? 300 : item ? item.implicitWidth : 0) : root.panelWidget ? 300 : root.widgetId.startsWith("control-") && root.container === "control-centre" ? 188 : item ? item.implicitWidth : 0
            height: root.panelWidget ? 240 : item ? item.implicitHeight : 0
            scale: Math.min(1, root.width / Math.max(1, width), root.height / Math.max(1, height))
            transformOrigin: Item.TopLeft
            sourceComponent: root.groupedWidget ? ({
                    "controls-header": groupPreview, "controls-audio": groupPreview, "controls-tiles": groupPreview,
                    "control-volume": audioControl, "control-microphone": audioControl, "control-media": mediaControl,
                    "control-divider": divider, "control-header-space": space, "control-battery": battery,
                    "control-customise": customiseButton
                })[root.widgetId] || headerButton : root.spec.decoration ? decoration : root.spec.layoutItem ? space : root.widgetId.startsWith("control-") ? control
                : ({search, clock, controls: indicators, notifications, metrics: monitor, keyboard, privacy, capture,
                    overflow, tray, notes, calendar, media, tasks, terminal, monitor: performance})[root.widgetId] || empty
        }
    }
    Component { id: headerButton; IconButton { iconName: root.spec.icon || ""; label: root.spec.label || ""; presentation: root.face } }
    Component { id: groupPreview; ControlGroupPreview { widgetId: root.widgetId } }
    Component {
        id: audioControl
        AudioLevel { node: sampleAudioNode; label: root.spec.label; iconName: root.spec.icon; navigation: true }
    }
    Component { id: mediaControl; ControlCentreMedia { player: samplePlayer; playerOptions: [samplePlayer]; active: false } }
    Component { id: divider; Rectangle { implicitWidth: root.container === "control-centre" ? 300 : 1; implicitHeight: root.container === "control-centre" ? 1 : Theme.barHeight - 12; color: Theme.outline; opacity: 0.5 } }
    Component { id: battery; BatteryStatus { available: true; summary: "Battery 84 percent, charging" } }
    Component { id: customiseButton; ActionButton {
        text: "Customise controls..."; iconName: "document-edit-symbolic"; flat: true
        implicitHeight: root.container === "control-centre" ? 28 : Theme.barHeight
        presentation: DesktopLayout.presentation(DesktopEditing.desktop, root.widgetId, root.container, text, iconName, root.container !== "control-centre", root.container === "control-centre")
    } }
    QtObject { id: sampleAudioNode; property bool ready: true; property var audio: QtObject { property real volume: 0.6; property bool muted: false } }
    Component {
        id: control
        ControlRow {
            readonly property string kind: root.widgetId.slice(8)
            barLayout: root.container !== "control-centre"
            title: root.spec.label || ""
            subtitle: ({network: "Home network", bluetooth: "Connected", vpn: "Connected", power: "Balanced"})[kind] || ""
            iconName: root.spec.icon || ""
            presentation: DesktopLayout.presentation(DesktopEditing.desktop, root.widgetId, root.container, title, iconName, true, !barLayout)
            tileLayout: !["vpn", "power", "awake"].includes(kind)
            tileSurface: !tileLayout
            compactTile: ["dnd", "nightLight"].includes(kind)
            selected: ["network", "bluetooth", "vpn"].includes(kind)
            navigation: ["network", "bluetooth", "vpn", "power"].includes(kind)
            toggleVisible: !["network", "vpn", "power"].includes(kind)
            toggleChecked: kind === "bluetooth"
            rowInteractive: false
        }
    }
    Component { id: decoration; DesktopDecoration { widgetId: root.widgetId } }
    Component { id: space; BarSpace {
        flexible: root.widgetId.startsWith("spring") || (root.widgetId === "control-header-space" && root.container === "control-centre" && DesktopEditing.desktop.widgetOptions?.[root.widgetId]?.width === undefined)
        gapSize: DesktopEditing.desktop.widgetOptions?.[root.widgetId]?.width || (root.widgetId === "control-header-space" ? 16 : 20)
        editing: true; implicitWidth: flexible ? 120 : gapSize
    } }
    Component { id: search; BarSearchButton { presentation: root.face } }
    Component {
        id: clock
        Pill {
            horizontalPadding: Theme.barPrimaryPadding
            presentation: root.face
            Text { text: "Tue 8 Sep"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
            Text { text: "12:30"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
        }
    }
    Component {
        id: indicators
        Pill {
            horizontalPadding: Theme.barPrimaryPadding
            presentation: root.face
            Row {
                Repeater {
                    model: ["network-wireless-signal-excellent-symbolic", "audio-volume-high-symbolic", "bluetooth-active-symbolic"]
                    StatusIndicator { required property string modelData; shown: true; iconName: modelData }
                }
            }
        }
    }
    Component { id: notifications; NotificationIndicator { count: 3 } }
    Component { id: capture; ActivityIndicator { barWindow: DesktopEditing.editor?.nativeWindow; filled: true; label: "00:24"; activityColor: Theme.recordingIndicator; trailingIcon: "screencast-stop-symbolic" } }
    Component { id: privacy; ActivityIndicator { barWindow: DesktopEditing.editor?.nativeWindow; iconName: "microphone-sensitivity-high-symbolic" } }
    Component { id: overflow; IconButton { iconName: "view-more-symbolic"; label: "More"; background: BarControlSurface {} } }
    Component {
        id: tray
        Row {
            spacing: Theme.gap
            Repeater { model: ["mail-unread-symbolic", "network-vpn-symbolic", "drive-harddisk-symbolic"]; SymbolicIcon { required property string modelData; implicitSize: Theme.iconSize; source: Quickshell.iconPath(modelData); color: Theme.text } }
        }
    }
    Component { id: keyboard; InputSourceSelector { parentWindow: DesktopEditing.editor?.nativeWindow; metrics: sampleMetrics; gnoblinCtlPath: ""; shortcutsEnabled: false; presentation: root.face } }
    Component { id: monitor; SystemMetrics { systemMetrics: sampleMetrics; presentation: root.face } }
    Component { id: performance; SidebarMonitor { metrics: sampleMetrics } }
    Component { id: notes; SidebarNotes { previewText: "# Weekend plans\n\nA few things to remember.\n\n- Pick up groceries\n- Book a table\n\n## Ideas\n\nKeep the afternoon free." } }
    Component { id: tasks; SidebarTasks { previewTasks: [{text: "Plan the week", done: false}, {text: "Book tickets", done: false}, {text: "Reply to messages", done: true}] } }
    Component { id: media; SidebarMedia { players: [samplePlayer] } }
    Component { id: calendar; SidebarCalendar { serviceEnabled: false; month: new Date(2026, 8, 1); selectedDate: new Date(2026, 8, 8) } }
    Component {
        id: terminal
        Text { text: "$ ls\nDocuments  Downloads  Music\nPictures   Projects   Videos\n\n$ "; color: Theme.text; font.family: "DejaVu Sans Mono"; font.pointSize: Theme.fontSize * 0.75; padding: 12 }
    }
    Component { id: empty; Item {} }
    QtObject {
        id: sampleMetrics
        property bool available: true
        property var sampleSnapshot: ({cpuPercent: 24, memoryUsedBytes: 8589934592, memoryTotalBytes: 34359738368,
            networkReceiveBytesPerSecond: 128000, networkTransmitBytesPerSecond: 32000,
            extra: {cpuTemperatureCelsius: 48, load1: 1.2, logicalCpus: 8, swapUsedBytes: 0, swapTotalBytes: 0, diskReadBytesPerSecond: 0, diskWriteBytesPerSecond: 0}})
        property var latest: sampleSnapshot
        property var history: []
        property string cpuLabel: "8-core processor"
        property bool desktopStateAvailable: true
        property string inputSourceLabel: "en"
        property var inputSources: [{type: "xkb", id: "gb", displayName: "English (UK)", shortName: "en"}]
        property var currentInputSource: inputSources[0]
        function formatBytes(value) { return (value / 1073741824).toFixed(0) + "G"; }
        function formatRate(value) { return (value / 1000).toFixed(0) + "K/s"; }
    }
    QtObject {
        id: samplePlayer
        property string identity: "Music"
        property string uniqueId: "preview-track"
        property string trackTitle: "A little music for the afternoon"
        property string trackArtist: "Sample artist"
        property string trackArtUrl: ""
        property real position: 72
        property real length: 224
        property bool isPlaying: false
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canSeek: true
        property bool canGoNext: true
        property bool canGoPrevious: true
        property bool positionSupported: true
        property bool lengthSupported: true
    }
    // Preview controls cannot launch apps, write notes, or change device state.
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
}
