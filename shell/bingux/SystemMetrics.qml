import QtQuick
import QtQuick.Layouts
import QtCore
import Quickshell

Pill {
    id: root
    required property var systemMetrics
    readonly property bool available: !!systemMetrics && systemMetrics.available
    readonly property var sample: available ? systemMetrics.sampleSnapshot ?? systemMetrics.latest : null
    readonly property bool cpuAvailable: sample !== null && sample.cpuPercent !== null
    readonly property real cpuFraction: cpuAvailable ? Math.max(0, Math.min(1, sample.cpuPercent / 100)) : 0
    readonly property real memoryFraction: sample && sample.memoryTotalBytes > 0
        ? Math.max(0, Math.min(1, sample.memoryUsedBytes / sample.memoryTotalBytes)) : 0
    readonly property var history: systemMetrics && systemMetrics.history ? systemMetrics.history : []
    property double chartTime: Date.now()
    onSampleChanged: chartTime = Date.now()
    onHistoryChanged: chartTime = Date.now()
    Timer { interval: 2000; repeat: true; running: root.visible; onTriggered: root.chartTime = Date.now() }
    property url preferencesLocation: "file://" + Quickshell.env("HOME") + "/.config/bingux/monitors.ini"
    Settings {
        id: preferences
        location: root.preferencesLocation
        property bool cpu: true
        property bool memory: true
        property bool receive: false
        property bool send: false
        property bool temperature: false
        property bool load: false
        property bool swap: false
        property bool diskRead: false
        property bool diskWrite: false
    }
    readonly property var monitorNames: ["cpu", "memory", "receive", "send", "temperature", "load", "swap", "diskRead", "diskWrite"]
    property var previewMonitors: null
    readonly property var selectedNames: monitorNames.filter(name => previewMonitors ? previewMonitors.includes(name) : isShown(name))
    function isShown(name) { return monitorNames.includes(name) && preferences[name] === true; }
    function setShown(name, shown) {
        if (!monitorNames.includes(name) || (!shown && isShown(name) && selectedNames.length <= 1)) return;
        preferences[name] = shown;
        preferences.sync();
    }
    function titleFor(name) { return {cpu: "Processor", memory: "Memory", receive: "Network receive", send: "Network send", temperature: "CPU temperature", load: "System load (1m)", swap: "Swap", diskRead: "Disk reads", diskWrite: "Disk writes"}[name]; }
    function numericFor(name, point) {
        if (point) return name === "memory" ? point.memoryUsed : name === "swap" ? point.swapUsed : point[name];
        if (!available || !sample) return null;
        const extra = sample.extra || {};
        return {cpu: sample.cpuPercent, memory: sample.memoryUsedBytes, receive: sample.networkReceiveBytesPerSecond,
            send: sample.networkTransmitBytesPerSecond, temperature: extra.cpuTemperatureCelsius, load: extra.load1,
            swap: extra.swapUsedBytes, diskRead: extra.diskReadBytesPerSecond, diskWrite: extra.diskWriteBytesPerSecond}[name];
    }
    function valueFor(name, point) {
        const value = numericFor(name, point);
        if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return "—";
        if (name === "cpu") return Math.round(value) + "%";
        if (name === "temperature") return Math.round(value) + "°C";
        if (name === "load") return value.toFixed(2);
        if (name === "memory" || name === "swap") return name === "swap" && (point ? point.swapTotal === 0 : sample.extra.swapTotalBytes === 0) ? "Off" : systemMetrics.formatBytes(value);
        return systemMetrics.formatRate(value);
    }
    function supported(name) { return !["temperature", "load", "swap", "diskRead", "diskWrite"].includes(name) || typeof numericFor(name) === "number"; }
    function maximumFor(name, peak) {
        if (["cpu", "memory", "swap"].includes(name)) return 100;
        if (name === "temperature") return 120;
        if (name === "load") return Math.max(1, sample && sample.extra && sample.extra.logicalCpus || 1, peak || 0);
        return 0;
    }
    readonly property string description: !available || !sample ? "System usage unavailable"
        : systemMetrics.cpuLabel + "\nMemory " + systemMetrics.formatBytes(sample.memoryUsedBytes)
            + " of " + systemMetrics.formatBytes(sample.memoryTotalBytes) + " · " + Math.round(memoryFraction * 100) + "%"
            + "\nNetwork receive " + valueFor("receive") + " · send " + valueFor("send")
    readonly property bool pointerHovered: mouse.containsMouse
    signal configureRequested()
    signal performanceRequested()
    interactive: true
    hovered: mouse.containsMouse
    pressed: mouse.pressed
    activeFocusOnTab: true
    horizontalPadding: Theme.barPrimaryPadding
    spacing: 12
    Accessible.role: Accessible.Button
    Accessible.name: "System monitors"
    Accessible.description: description
    Accessible.onPressAction: performanceRequested()
    Keys.onReturnPressed: performanceRequested()
    Keys.onSpacePressed: performanceRequested()
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))) {
            root.configureRequested();
            event.accepted = true;
        }
    }
    MouseArea {
        id: mouse
        parent: root
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: event => event.button === Qt.RightButton ? root.configureRequested() : root.performanceRequested()
    }

    GridLayout {
        id: readouts
        Layout.fillWidth: root.panelLayout
        readonly property real widestReadout: {
            let width = 1;
            for (let index = 0; index < metricsRepeater.count; index++)
                width = Math.max(width, metricsRepeater.itemAt(index)?.implicitWidth || 0);
            return width;
        }
        columns: root.panelLayout ? Math.max(1, Math.floor((root.width - root.horizontalPadding * 2 + columnSpacing)
            / (widestReadout + columnSpacing))) : -1
        rows: root.panelLayout ? Math.ceil(root.selectedNames.length / columns) : 2
        flow: GridLayout.TopToBottom
        rowSpacing: root.panelLayout ? Theme.spaceSmall : 0
        columnSpacing: 14
        Repeater {
            id: metricsRepeater
            model: root.selectedNames
            Readout {
                required property string modelData
                objectName: modelData + "Readout"
                Layout.fillWidth: true
                label: ({cpu: "CPU", memory: "RAM", receive: "↓", send: "↑", temperature: "Temp", load: "Load", swap: "Swap", diskRead: "R", diskWrite: "W"})[modelData]
                value: root.valueFor(modelData)
                metric: modelData
                reservedValue: ({cpu: "100%", memory: "999G", receive: "999M/s", send: "999M/s",
                    temperature: "100°C", load: "99.99", swap: "999G", diskRead: "999M/s", diskWrite: "999M/s"})[modelData]
            }
        }
    }

    component Readout: Item {
        id: readout
        required property string label
        required property string value
        required property string metric
        required property string reservedValue
        property real widestValue: 0
        readonly property real valueWidth: Math.ceil(Math.max(valueMeasure.advanceWidth, widestValue))
        implicitWidth: caption.implicitWidth + 4 + valueWidth + 6 + 42
        implicitHeight: 14
        TextMetrics {
            id: valueMeasure
            text: readout.reservedValue
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.features: ({"tnum": 1})
        }
        TextMetrics {
            text: readout.value
            font: valueMeasure.font
            onAdvanceWidthChanged: readout.widestValue = Math.max(readout.widestValue, advanceWidth)
        }
        Text {
            id: caption
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: readout.label
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
        }
        RollingNumber {
            id: reading
            width: readout.valueWidth
            height: readout.height
            anchors.left: caption.right
            anchors.leftMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            text: readout.value
            value: root.numericFor(readout.metric) ?? NaN
            pixelSize: Theme.fontSmall
            fontWeight: Font.DemiBold
            objectName: readout.metric + "BarValue"
        }
        MetricGraph {
            id: miniChart
            objectName: readout.metric + "BarGraph"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: 42
            height: 12
            history: root.history
            endTime: root.chartTime
            duration: Math.min(60000, Math.max(10000, root.history.length ? root.chartTime - root.history[0].at : 60000))
            metric: readout.metric
            fixedMaximum: root.maximumFor(readout.metric, miniChart.peak)
            lineColor: readout.metric === "cpu" || readout.metric === "receive" ? Theme.accent : Theme.text
        }
    }
}
