import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Window
import Quickshell.Io
import Quickshell
import "ProcessApplications.js" as ProcessApplications

Item {
    id: root
    required property var monitorWidget
    readonly property var hardware: monitorWidget.available && monitorWidget.sample ? monitorWidget.sample.extra?.hardware ?? null : null
    property string page: "gpu"
    property var processRecords: []
    property double processRevision: -1
    function updateProcesses() {
        if (!visible || page !== "processes" || !hardware) { processRecords = []; processRevision = -1; return; }
        const revision = hardware.processesSampledAtMs;
        if (revision === undefined || revision !== processRevision) {
            processRevision = revision ?? -1;
            processRecords = hardware.processes ?? [];
        }
    }
    onHardwareChanged: updateProcesses()
    property string processSort: "CPU"
    onProcessSortChanged: if (processLoader.item) processLoader.item.sortKey = processSort
    readonly property var processes: processLoader.item?.sortedRecords ?? []
    implicitHeight: root.page === "gpu" ? gpuSection.implicitHeight : root.page === "storage" ? storageSection.implicitHeight : 360
    function number(value, unit, digits = 0) { return typeof value === "number" && isFinite(value) ? value.toFixed(digits) + unit : "—"; }
    function bytes(value) { return typeof value === "number" ? monitorWidget.systemMetrics.formatBytes(value) : "—"; }
    ColumnLayout { id: gpuSection; visible: root.page === "gpu"; width: parent.width; spacing: 10
    Header { title: "GPU"; iconName: "video-display-symbolic"; expandable: false }
    Caption { visible: !(root.hardware?.gpus?.length); text: "Unavailable" }
    Repeater {
        model: root.hardware?.gpus ?? []
        ColumnLayout {
            required property var modelData
            Layout.fillWidth: true; spacing: 10
            Caption { text: modelData.name; color: Theme.text }
            Reading { label: "Usage"; reading: root.number(modelData.busyPercent, "%"); readingColor: Theme.usageColor(modelData.busyPercent) }
            Reading { label: "Temperature"; reading: root.number(modelData.temperatureCelsius, "°C") }
            Reading { label: "VRAM"; readingColor: Theme.usageColor(modelData.memoryTotalBytes > 0 ? modelData.memoryUsedBytes / modelData.memoryTotalBytes * 100 : null); reading: root.bytes(modelData.memoryUsedBytes) + " / " + root.bytes(modelData.memoryTotalBytes) }
            Meter { visible: typeof modelData.memoryUsedBytes === "number" && modelData.memoryTotalBytes > 0; fraction: modelData.memoryTotalBytes > 0 ? modelData.memoryUsedBytes / modelData.memoryTotalBytes : 0 }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 10
                Reading { label: "Power"; reading: root.number(modelData.powerWatts, " W", 1) }
                Reading { label: "Clock"; reading: root.number(modelData.clockMhz, " MHz") }
                Reading { label: "Fan"; reading: root.number(modelData.fanRpm, " RPM") }
                Reading { label: "Driver"; reading: modelData.driver || "—" }
                Reading { label: "PCI"; reading: modelData.pciAddress }
            }
        }
    }
    }
    ColumnLayout { id: storageSection; visible: root.page === "storage"; width: parent.width; spacing: 10
    Header { title: "Storage"; iconName: "drive-harddisk-symbolic"; expandable: false }
    Repeater {
        model: root.hardware?.storage ?? []
        ColumnLayout {
            required property var modelData
            Layout.fillWidth: true; spacing: 5
            Reading { label: modelData.path === "/" ? "System" : "Home"; reading: root.bytes(modelData.availableBytes) + " free"; readingColor: Theme.usageColor(modelData.totalBytes > 0 ? (1 - modelData.availableBytes / modelData.totalBytes) * 100 : null) }
            Meter { fraction: modelData.totalBytes > 0 ? 1 - modelData.availableBytes / modelData.totalBytes : 0 }
            HoverHandler { id: storageHover }
            ShellTooltip { visible: storageHover.hovered; text: modelData.path + " · " + root.bytes(modelData.totalBytes) + " total" }
        }
    }
    }
    Loader {
        id: processLoader
        active: root.page === "processes"
        anchors.fill: parent
        anchors.bottomMargin: actionNotice.visible ? actionNotice.implicitHeight + 10 : 0
        sourceComponent: ProcessTable {
            objectName: "processTable"
            records: root.processRecords
            totalCount: root.hardware?.processCount ?? 0
            applicationFor: root.applicationFor
            formatBytes: root.bytes
            sortKey: root.processSort
            onSortKeyChanged: root.processSort = sortKey
            onContextRequested: (row, processes) => root.openProcessMenu(row, processes)
        }
    }
    property var selectedProcesses: []
    readonly property var selectedProcess: selectedProcesses[0] || null
    property string actionMessage: ""
    readonly property var applicationIndex: ProcessApplications.index(ApplicationCatalog.entries)
    function applicationFor(process) {
        return ProcessApplications.lookup(process, applicationIndex, DesktopEntries);
    }
    function openProcessMenu(row, processes) {
        selectedProcesses = processes.map(process => Object.assign({}, process));
        const point = row.mapToItem(processMenu.contentItem, 0, row.height);
        processMenu.preferredX = point.x;
        processMenu.preferredY = point.y;
        processMenu.visible = true;
        Qt.callLater(() => processActions.children[1].forceActiveFocus());
    }
    function processAction(action) {
        if (!selectedProcess || actionRunner.running) return;
        processMenu.visible = false;
        actionMessage = "";
        actionRunner.command = ["python3", decodeURIComponent(Qt.resolvedUrl("process-action.py").toString().replace(/^file:\/\//, "")),
            "--batch", action];
        actionRunner.running = true;
    }
    onPageChanged: { processMenu.visible = false; updateProcesses(); }
    onVisibleChanged: { if (!visible) processMenu.visible = false; updateProcesses(); }
    Process {
        id: actionRunner
        stdinEnabled: true
        onStarted: write(JSON.stringify(root.selectedProcesses.map(process => ({pid: process.pid, startTime: process.startTime || 0}))) + "\n")
        stdout: StdioCollector { onStreamFinished: root.actionMessage = text.trim() }
        stderr: StdioCollector { onStreamFinished: if (text.trim()) root.actionMessage = text.trim() }
    }
    Caption { id: actionNotice; anchors.bottom: parent.bottom; width: parent.width; visible: root.page === "processes" && text.length > 0; text: root.actionMessage }
    ShellPopup {
        id: processMenu
        objectName: "processContextMenu"
        hostItem: root.Window.window ? root.Window.window.contentItem : null
        popupWidth: 216
        contentPadding: 6
        ColumnLayout {
            id: processActions
            width: parent.width
            spacing: 2
            Caption { Layout.margins: 6; text: root.selectedProcesses.length > 1 ? root.selectedProcesses.length + " processes selected" : root.selectedProcess ? root.selectedProcess.name + " · " + root.selectedProcess.pid : "" }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: root.selectedProcesses.length > 1 ? "Copy PIDs" : "Copy PID"; iconName: "edit-copy-symbolic"; onClicked: root.processAction("copy") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: "Pause"; objectName: "processAction_pause"; iconName: "media-playback-pause-symbolic"; enabled: !actionRunner.running && root.selectedProcesses.length > 0 && root.selectedProcesses.every(process => !!process.startTime); onClicked: root.processAction("pause") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: "Resume"; objectName: "processAction_resume"; iconName: "media-playback-start-symbolic"; enabled: !actionRunner.running && root.selectedProcesses.length > 0 && root.selectedProcesses.every(process => !!process.startTime); onClicked: root.processAction("resume") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: root.selectedProcesses.length > 1 ? "End processes" : "End process"; objectName: "processAction_end"; iconName: "media-playback-stop-symbolic"; enabled: !actionRunner.running && root.selectedProcesses.length > 0 && root.selectedProcesses.every(process => !!process.startTime); onClicked: root.processAction("end") }
            ActionButton { Layout.fillWidth: true; flat: true; alignLeft: true; text: "Force quit"; objectName: "processAction_kill"; iconName: "process-stop-symbolic"; enabled: !actionRunner.running && root.selectedProcesses.length > 0 && root.selectedProcesses.every(process => !!process.startTime); onClicked: root.processAction("kill") }
            Keys.onPressed: event => {
                if (![Qt.Key_Up, Qt.Key_Down].includes(event.key)) return;
                const buttons = children.filter(child => typeof child.clicked === "function" && child.enabled);
                const index = buttons.findIndex(child => child.activeFocus);
                buttons[(index + (event.key === Qt.Key_Down ? 1 : buttons.length - 1)) % buttons.length].forceActiveFocus();
                event.accepted = true;
            }
        }
    }
    component Caption: Text {
        property bool heading: false
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        wrapMode: Text.WrapAnywhere
        textFormat: Text.PlainText
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: heading ? Font.DemiBold : Font.Normal
    }
    component Header: RowLayout {
        id: header
        property string title
        property string iconName
        property bool expandable: true
        property bool expanded: false
        signal toggled()
        Layout.fillWidth: true
        spacing: 8
        SymbolicIcon { implicitSize: 16; source: Quickshell.iconPath(header.iconName); color: Theme.muted }
        Caption { text: header.title; color: Theme.text; heading: true }
        IconButton {
            visible: header.expandable
            implicitWidth: 24; implicitHeight: 24
            iconName: header.expanded ? "pan-up-symbolic" : "pan-down-symbolic"
            label: (header.expanded ? "Hide " : "Show ") + header.title.toLowerCase() + " details"
            onClicked: header.toggled()
        }
    }
    component Reading: RowLayout {
        property color readingColor: Theme.text
        property string label
        property string reading
        Layout.fillWidth: true
        spacing: 8
        Caption { text: parent.label; Layout.preferredWidth: 1 }
        Caption { text: parent.reading; Layout.preferredWidth: 2; horizontalAlignment: Text.AlignRight; color: parent.readingColor; font.features: ({"tnum": 1}) }
    }
    component Meter: Rectangle {
        property real fraction: 0
        Layout.fillWidth: true
        implicitHeight: 3
        radius: 1.5
        color: Theme.hover
        Rectangle { width: parent.width * Math.max(0, Math.min(1, parent.fraction)); height: parent.height; radius: parent.radius; color: Theme.usageColor(parent.fraction * 100, Theme.accent) }
    }
    component Divider: Rectangle { Layout.fillWidth: true; Layout.topMargin: 8; Layout.bottomMargin: 6; implicitHeight: 1; color: Theme.hover }
}
