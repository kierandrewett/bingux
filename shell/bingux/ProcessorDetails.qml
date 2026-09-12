import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var monitorWidget
    property int duration: 60000
    property alias headerRow: header
    readonly property var hardware: monitorWidget.sample?.extra?.hardware
    readonly property var cores: monitorWidget.systemMetrics.latest?.extra?.cpuCores ?? []
    readonly property string coreIdentity: cores.map(core => core.id).join(",")
    readonly property var coreIds: coreIdentity ? coreIdentity.split(",").map(Number) : []
    spacing: 14
    RowLayout {
        id: header
        Layout.fillWidth: true
        Text {
            Layout.fillWidth: true
            text: "Processor"
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
        }
    }
    Text {
        Layout.fillWidth: true
        text: root.hardware?.cpuModel || "CPU"
        wrapMode: Text.Wrap
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    GridLayout {
        Layout.fillWidth: true
        columns: root.width >= 220 ? 2 : 1
        RollingNumber {
            Layout.fillWidth: true
            text: root.monitorWidget.valueFor("cpu")
            value: root.monitorWidget.sample?.cpuPercent ?? NaN
            color: Theme.usageColor(value)
            pixelSize: 24
        }
        Text {
            Layout.fillWidth: true
            text: root.hardware?.cpuMhz ? (root.hardware.cpuMhz / 1000).toFixed(2) + " GHz" : "—"
            horizontalAlignment: root.width >= 220 ? Text.AlignRight : Text.AlignLeft
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 18
        }
    }
    Text {
        Layout.fillWidth: true
        text: root.cores.length ? root.cores.length + " logical processors · 0–100%" : "Waiting for per-core readings…"
        wrapMode: Text.Wrap
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    GridLayout {
        Layout.fillWidth: true
        columns: Math.max(1, Math.min(4, Math.floor(root.width / 100)))
        columnSpacing: 10
        rowSpacing: 14
        Repeater {
            model: root.coreIds
            ColumnLayout {
                id: coreCell
                required property int modelData
                readonly property var sample: root.cores.find(core => core.id === modelData)
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                spacing: 3
                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: "CPU " + coreCell.modelData
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 10
                    }
                    Text {
                        readonly property var reading: graph.inspectedPoint ? graph.inspectedPoint["cpu" + coreCell.modelData] : root.monitorWidget.available ? coreCell.sample?.usage : null
                        text: typeof reading === "number" ? Math.round(reading) + "%" : "—"
                        color: Theme.usageColor(reading)
                        font.family: Theme.fontFamily
                        font.pixelSize: 10
                        font.features: ({
                                "tnum": 1
                            })
                    }
                }
                MetricGraph {
                    id: graph
                    objectName: "logicalCpuGraph_" + coreCell.modelData
                    Layout.fillWidth: true
                    implicitHeight: 54
                    history: root.monitorWidget.history
                    metric: "cpu" + coreCell.modelData
                    fixedMaximum: 100
                    duration: root.duration
                    endTime: root.monitorWidget.chartTime
                    detailed: true
                    Rectangle {
                        anchors.fill: parent
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.hover
                    }
                }
            }
        }
    }
    ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: 4
        spacing: 10
        Detail {
            label: "Temperature"
            reading: typeof root.monitorWidget.sample?.extra?.cpuTemperatureCelsius === "number" ? Math.round(root.monitorWidget.sample.extra.cpuTemperatureCelsius) + "°C" : "—"
        }
        Detail {
            label: "Logical processors"
            reading: String(root.monitorWidget.sample?.extra?.logicalCpus ?? "—")
        }
        Detail {
            label: "Load average"
            reading: [root.monitorWidget.sample?.extra?.load1, root.monitorWidget.sample?.extra?.load5, root.monitorWidget.sample?.extra?.load15].map(value => typeof value === "number" ? value.toFixed(2) : "—").join(" / ")
        }
        Detail {
            label: "Uptime"
            reading: root.uptime(root.hardware?.uptimeSeconds)
        }
        Detail {
            label: "Kernel"
            reading: root.hardware?.kernel || "—"
        }
    }
    function uptime(value) {
        if (typeof value !== "number")
            return "—";
        const days = Math.floor(value / 86400);
        return (days ? days + "d " : "") + Math.floor(value / 3600) % 24 + "h " + Math.floor(value / 60) % 60 + "m";
    }
    component Detail: RowLayout {
        property string label
        property string reading
        Layout.fillWidth: true
        spacing: 8
        Text {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            wrapMode: Text.Wrap
            text: parent.label
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        Text {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            wrapMode: Text.WrapAnywhere
            text: parent.reading
            horizontalAlignment: Text.AlignRight
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
}
