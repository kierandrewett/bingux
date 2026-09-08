import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

Item {
    id: root
    required property var monitorWidget
    property bool compact: false
    property string footerText: ""
    property string range: "1m"
    property string page: "usage"
    property real availableHeight: 0
    implicitHeight: tabs.implicitHeight + spacing + (root.page !== "usage" ? 360 : pageContent.implicitHeight)
    readonly property int duration: range === "5m" ? 300000 : 60000
    property real spacing: root.compact ? 16 : 24
    GridLayout {
        id: tabs
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        columns: 1
        rowSpacing: 10
        GridLayout {
            Layout.fillWidth: true
            columns: root.width < 210 ? 2 : 3
            columnSpacing: 0; rowSpacing: 0
            Repeater {
                model: ["usage", "processes", "services"]
                AbstractButton {
                    id: pageButton
                    required property string modelData
                    objectName: "performancePage_" + modelData
                    text: ({usage: "Overview", processes: "Processes", services: "Services"})[modelData]
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    Layout.preferredWidth: 1
                    implicitWidth: label.implicitWidth + (root.width < 180 ? 8 : 16)
                    implicitHeight: 32
                    hoverEnabled: true
                    checked: root.page === modelData
                    Accessible.role: Accessible.PageTab
                    Accessible.name: text
                    Accessible.checkable: true
                    Accessible.checked: checked
                    onClicked: root.page = modelData
                    background: Rectangle {
                        radius: 5
                        color: pageButton.hovered || pageButton.visualFocus ? Theme.hover : "transparent"
                        Rectangle {
                            anchors.bottom: parent.bottom
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: label.implicitWidth
                            height: 2
                            radius: 1
                            color: Theme.text
                            opacity: pageButton.checked ? 1 : 0
                            Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 120 } }
                        }
                    }
                    contentItem: Text {
                        id: label
                        text: pageButton.text
                        color: pageButton.checked ? Theme.text : Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        font.weight: pageButton.checked ? Font.DemiBold : Font.Normal
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }

    }
    SegmentedControl {
        parent: processorDetails.headerRow
            visible: root.page === "usage"
            Layout.alignment: Qt.AlignRight
            Layout.minimumWidth: 88
            Layout.maximumWidth: 88
            options: ["1m", "5m"]
            currentValue: root.range
            accessiblePrefix: "History range "
            objectNamePrefix: "monitorRange_"
            implicitWidth: 88
            implicitHeight: 28
            buttonRadius: 4
            onSelected: value => root.range = value
        }
    ScrollView {
        id: pageScroll
        objectName: "monitorPageScroll"
        anchors.top: tabs.bottom
        anchors.topMargin: root.spacing
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        clip: true
        contentWidth: availableWidth
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ScrollBar.vertical.policy: root.page !== "usage" ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded
        Binding { target: pageScroll.contentItem; property: "boundsBehavior"; value: Flickable.StopAtBounds }
        Binding { target: pageScroll.contentItem; property: "boundsMovement"; value: Flickable.StopAtBounds }
        ColumnLayout {
            id: pageContent
            width: pageScroll.availableWidth
            height: root.page !== "usage" ? pageScroll.availableHeight : implicitHeight
            spacing: root.spacing
    Text {
        visible: !root.monitorWidget.available
        Layout.fillWidth: true; wrapMode: Text.Wrap
        text: "Waiting for live readings…"
        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
    }
    ProcessorDetails {
        id: processorDetails
        objectName: "processorDetails"
        visible: root.page === "usage"
        Layout.fillWidth: true
        monitorWidget: root.monitorWidget
        duration: root.duration
    }
    GridLayout {
        id: overviewGrid
        visible: root.page === "usage"
        Layout.fillWidth: true
        columns: root.width >= 500 ? 2 : 1
        columnSpacing: 24; rowSpacing: 24
        ColumnLayout {
            Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.alignment: Qt.AlignTop
            spacing: 12
            UsageGraph { metric: "memory"; title: "Memory" }
            UsageGraph { metric: "swap"; title: "Swap"; Layout.topMargin: 8 }
            Text {
                visible: typeof root.monitorWidget.sample?.extra?.swapTotalBytes === "number"
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: root.monitorWidget.systemMetrics.formatBytes(root.monitorWidget.sample?.extra?.swapTotalBytes ?? 0) + " swap capacity"
                color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
            }
        }
        HardwareDetails { Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.alignment: Qt.AlignTop; page: "gpu"; monitorWidget: root.monitorWidget }
        ColumnLayout {
            Layout.fillWidth: true; Layout.columnSpan: overviewGrid.columns
            spacing: 20
            HardwareDetails { Layout.fillWidth: true; page: "storage"; monitorWidget: root.monitorWidget }
            TrafficGraph { id: diskTraffic; title: "Disk activity"; readMetric: "diskRead"; writeMetric: "diskWrite" }
        }
    }
    HardwareDetails {
        objectName: "hardwareDetails"
        visible: root.page === "processes"
        page: "processes"
        Layout.fillWidth: true; Layout.fillHeight: true
        Layout.minimumHeight: 120
        monitorWidget: root.monitorWidget
    }
    ServicesPanel { objectName: "servicesPanel"; visible: root.page === "services"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.minimumHeight: 160 }
    TrafficGraph { id: networkTraffic; visible: root.page === "usage"; title: "Network"; readMetric: "receive"; writeMetric: "send" }
        }
    }
    onPageChanged: if (pageScroll.contentItem) pageScroll.contentItem.contentY = 0
    component TrafficGraph: ColumnLayout {
        id: traffic
        required property string title
        required property string readMetric
        required property string writeMetric
        Layout.fillWidth: true
        spacing: 8
        property alias headerRow: trafficHeader
        RowLayout {
            id: trafficHeader
            Layout.fillWidth: true
            Text { Layout.fillWidth: true; wrapMode: Text.Wrap; text: traffic.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.Medium }
            Text { visible: !root.compact && traffic.readMetric !== "diskRead"; text: root.monitorWidget.systemMetrics.formatRate(network.maximum) + " max"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: root.compact && root.width < 220 ? 1 : 2
            columnSpacing: 16
            RollingNumber {
                Layout.fillWidth: true
                prefix: traffic.readMetric === "receive" ? "↓ " : "R "
                text: root.monitorWidget.valueFor(traffic.readMetric, network.inspectedPoint)
                value: root.monitorWidget.numericFor(traffic.readMetric, network.inspectedPoint) ?? NaN
                color: Theme.accent
                pixelSize: 18
            }
            RollingNumber {
                prefix: traffic.writeMetric === "send" ? "↑ " : "W "
                text: root.monitorWidget.valueFor(traffic.writeMetric, network.inspectedPoint)
                value: root.monitorWidget.numericFor(traffic.writeMetric, network.inspectedPoint) ?? NaN
                color: Theme.muted
                pixelSize: 18
            }
        }
        MetricGraph {
            id: network
            objectName: traffic.readMetric === "receive" ? "networkHistory" : "diskHistory"
            Layout.fillWidth: true
            implicitHeight: root.compact ? 72 : 104
            history: root.monitorWidget.history
            endTime: root.monitorWidget.chartTime
            duration: root.duration
            metric: traffic.readMetric
            secondaryMetric: traffic.writeMetric
            fixedMaximum: 0
            detailed: true
        }
        RowLayout {
            Layout.fillWidth: true
            Text { Layout.fillWidth: true; text: root.range === "5m" ? "5 minutes ago" : "1 minute ago"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
            Text { text: network.inspectedPoint ? Math.max(0, Math.round((root.monitorWidget.chartTime - network.inspectedPoint.at) / 1000)) + "s ago" : "Now"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
        }
    }
    component UsageGraph: ColumnLayout {
        id: usage
        required property string metric
        required property string title
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        spacing: 6
        readonly property var point: chart.inspectedPoint
        readonly property var utilisation: {
            if (point) return point[metric];
            const sample = root.monitorWidget.sample;
            if (!root.monitorWidget.available || !sample) return null;
            if (metric === "memory") return sample.memoryTotalBytes > 0 ? sample.memoryUsedBytes / sample.memoryTotalBytes * 100 : null;
            if (metric === "swap") return sample.extra?.swapTotalBytes > 0 ? sample.extra.swapUsedBytes / sample.extra.swapTotalBytes * 100 : null;
            return null;
        }
        property alias headerRow: usageHeader
        RowLayout {
            id: usageHeader
            Layout.fillWidth: true
            Layout.minimumHeight: 28
            spacing: 6
            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; wrapMode: Text.Wrap; text: usage.title; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
        }
        RollingNumber {
            objectName: usage.metric + "HistoryValue"
            Layout.fillWidth: true
            text: root.monitorWidget.valueFor(usage.metric, usage.point)
            value: root.monitorWidget.numericFor(usage.metric, usage.point) ?? NaN
            color: Theme.usageColor(usage.utilisation)
            pixelSize: 26
        }
        Text {
            visible: root.compact && usage.metric === "memory"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: usage.metric === "memory" && root.monitorWidget.sample
                ? root.monitorWidget.systemMetrics.formatBytes(root.monitorWidget.sample.memoryTotalBytes) + " total · "
                    + root.monitorWidget.systemMetrics.formatBytes(root.monitorWidget.sample.memoryTotalBytes - root.monitorWidget.sample.memoryUsedBytes) + " available"
                : "Usage over the last " + (root.range === "5m" ? "5 minutes" : "minute")
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        MetricGraph {
            id: chart
            objectName: usage.metric + "HistoryGraph"
            Layout.fillWidth: true
            implicitHeight: root.compact ? 56 : 80
            history: root.monitorWidget.history
            endTime: root.monitorWidget.chartTime
            duration: root.duration
            metric: usage.metric
            fixedMaximum: root.monitorWidget.maximumFor(usage.metric, chart.peak)
            detailed: true
            lineColor: usage.metric === "cpu" ? Theme.accent : Theme.muted
        }
        RowLayout {
            Layout.fillWidth: true
            Text { Layout.fillWidth: true; text: usage.metric === "temperature" ? "0–120°C" : usage.metric === "load" ? "0–" + Math.ceil(chart.maximum) + " tasks" : "0–100%"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
            Text { text: "Peak " + (usage.metric === "load" ? chart.peak.toFixed(2) : Math.round(chart.peak)) + (usage.metric === "temperature" ? "°C" : usage.metric === "load" ? "" : "%"); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
        }
    }
}
