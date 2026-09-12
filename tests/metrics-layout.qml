import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import "ControlLayout.js" as ControlLayout

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    QtObject {
        id: editor
        property bool visible: true
        property var nativeWindow: window
        property var desktop: ({
                controlLayout: ControlLayout.defaults()
            })
        property var layout: ({
                sidebar: ["metrics"]
            })
    }
    QtObject {
        id: sample
        property bool available: true
        property var latest: ({
                cpuPercent: 24,
                memoryUsedBytes: 12.4 * 1073741824,
                memoryTotalBytes: 32 * 1073741824,
                networkReceiveBytesPerSecond: 240000,
                networkTransmitBytesPerSecond: 32000,
                extra: {
                    cpuCores: Array.from({
                        length: 16
                    }, (_, id) => ({
                                id,
                                usage: id * 5
                            })),
                    cpuTemperatureCelsius: 54.2,
                    load1: 2.35,
                    logicalCpus: 16,
                    swapUsedBytes: 1073741824,
                    swapTotalBytes: 8589934592,
                    diskReadBytesPerSecond: 24000000,
                    diskWriteBytesPerSecond: 1200000
                }
            })
        property var history: []
        readonly property string cpuLabel: "CPU " + latest.cpuPercent + "%"
        function formatRate(rate) {
            return Math.round(rate / 1024) + "K/s";
        }
        function formatBytes(bytes) {
            return (bytes / 1073741824).toFixed(1) + "G";
        }
    }
    FloatingWindow {
        id: window
        implicitWidth: 900
        implicitHeight: 500
        color: Theme.background
        Item {
            id: panel
            width: 208
            height: 450
        }
        WidgetPreview {
            id: preview
            x: 400
            width: 320
            height: 240
            widgetId: "metrics"
        }
        SystemMetrics {
            id: monitor
            parent: panel
            systemMetrics: sample
            preferencesLocation: Qt.resolvedUrl("monitors.ini")
            property int activations: 0
            onPerformanceRequested: activations++
        }
        TestCase {
            when: window.visible
            function initTestCase() {
                report.setText("RUNNING");
                DesktopEditing.editor = editor;
                DesktopEditing.registerSource("metrics", monitor);
            }
            function cleanupTestCase() {
                DesktopEditing.unregisterSource("metrics", monitor);
                DesktopEditing.editor = null;
                report.setText("FAILURES " + qtest_results.failCount);
            }
            function test_all_readouts_fit() {
                try {
                    for (const name of monitor.monitorNames)
                        monitor.setShown(name, true);
                    wait(100);
                    const barWidth = monitor.width;
                    const barHeight = monitor.height;
                    const readoutGrid = findChild(monitor, "metricReadoutGrid");
                    compare(monitor.implicitWidth, readoutGrid.implicitWidth + monitor.horizontalPadding * 2, "The natural width matches the original two-row grid");
                    if ("panelLayout" in monitor)
                        monitor.panelLayout = true;
                    for (const width of [208, 376, 148, 208]) {
                        panel.width = width;
                        tryVerify(() => monitor.width <= panel.width, 1000, "All selected monitors fit the panel width");
                        compare(monitor.implicitWidth, barWidth, "Wrapping does not change the bar width budget");
                        for (const name of monitor.monitorNames) {
                            const readout = findChild(monitor, name + "Readout");
                            tryVerify(() => {
                                const point = readout.mapToItem(monitor, 0, 0);
                                return point.x >= 0 && point.y >= 0 && point.x + readout.width <= monitor.width + 0.01 && point.y + readout.height <= monitor.height + 0.01;
                            }, 1000, name + " stays inside the panel");
                            const graph = findChild(readout, name + "BarGraph");
                            const value = findChild(readout, name + "BarValue");
                            verify(graph.x >= value.x + value.width, "Charts do not overlap their values");
                        }
                        verify(monitor.height > barHeight, "All nine readouts wrap into the panel");
                    }
                    mouseClick(monitor, monitor.width / 2, monitor.height / 2);
                    compare(monitor.activations, 1);
                    monitor.presentation = {
                        custom: true,
                        showText: true,
                        showIcon: true,
                        label: "A very long monitor label that must fit its container",
                        icon: "computer-symbolic"
                    };
                    const label = findChild(monitor, "widgetFaceLabel");
                    tryVerify(() => label.truncated, 1000, "Long presentation overrides use an ellipsis");
                    monitor.presentation = null;
                    monitor.panelLayout = false;
                    tryCompare(monitor, "width", barWidth);
                    tryCompare(monitor, "height", barHeight);
                } catch (error) {
                    console.error("METRICS_FIT_FAILED", error.message, error.stack);
                    throw error;
                }
            }
            function test_preview_selection() {
                monitor.panelLayout = true;
                tryVerify(() => preview.previewControl !== null, 3000);
                const sample = preview.previewControl;
                verify(sample.panelLayout);
                tryCompare(sample, "width", monitor.width);
                compare(JSON.stringify(sample.selectedNames), JSON.stringify(monitor.selectedNames));
                monitor.setShown("diskWrite", false);
                tryVerify(() => !sample.selectedNames.includes("diskWrite"));
                monitor.setShown("diskWrite", true);
                tryVerify(() => sample.selectedNames.includes("diskWrite"));
            }
        }
    }
}
