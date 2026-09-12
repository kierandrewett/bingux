    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector {
            onStreamFinished: if (text)
                console.warn("NATIVE_INPUT", text)
        }
        property var gestureArguments: []
        property string capturePath: ""
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
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
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, options) {
            let frame = null;
            verify(item.grabToImage(result => frame = result));
            tryVerify(() => frame !== null, 4000);
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0);
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
        }
        function test_moved_monitors() {
            try {
                metricsPill.preferencesLocation = Qt.resolvedUrl("metrics-panel.ini");
                metricsPill.systemMetrics = sample;
                const now = Date.now();
                sample.history = Array.from({
                    length: 60
                }, (_, index) => ({
                            at: now - (59 - index) * 1000,
                            cpu: 20 + index / 2,
                            memory: 40 + index / 10,
                            memoryUsed: 12.4 * 1073741824,
                            receive: 100000 + index * 2000,
                            send: 32000 + index * 100,
                            temperature: 50 + index / 10,
                            load: 1 + index / 60,
                            swap: 12.5,
                            swapUsed: 1073741824,
                            diskRead: 20000000 + index * 20000,
                            diskWrite: 1000000 + index * 2000
                        }));
                for (const name of metricsPill.monitorNames)
                    metricsPill.setShown(name, true);
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                for (const container of ["sidebar", "control-centre"]) {
                    desktopCustomiser.open();
                    tryCompare(binguxSettings, "busy", false, 4000);
                    tryCompare(controlCentre, "revealScale", 1, 3000);
                    desktopCustomiser.put("metrics", container, 0);
                    desktopCustomiser.selectedContainer = container;
                    desktopCustomiser.containerDisplay("native");
                    save();
                    if (container === "sidebar") {
                        terminalSidebar.open();
                        tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                    } else {
                        terminalSidebar.hide();
                        tryCompare(terminalSidebar.editWindow, "reveal", 0, 3000);
                        controlCentre.visible = true;
                        tryCompare(controlCentre, "revealScale", 1, 3000);
                    }
                    const host = container === "sidebar" ? terminalSidebar.widgetHost : controlCentre.widgetHost;
                    const window = topBar.windowFor(metricsPill);
                    verify(metricsPill.parent === host);
                    verify(metricsPill.panelLayout);
                    tryVerify(() => metricsPill.width <= host.width && metricsPill.height > Theme.barHeight, 1000);
                    for (const name of metricsPill.monitorNames) {
                        const readout = findChild(metricsPill, name + "Readout");
                        tryVerify(() => readout.mapToItem(metricsPill, readout.width, 0).x <= metricsPill.width + 0.01, 1000);
                    }
                    nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-" + container + ".png" : "";
                    gesture(metricsPill, window, ["--hover-only"]);
                    nativeInput.capturePath = "";
                    gesture(metricsPill, window, ["--click-only"]);
                    tryCompare(metricsPopup, "visible", true, 3000);
                    verify(!metricsPopup.customising);
                    verify(metricsPopup.anchorWindow === window);
                    metricsPopup.visible = false;
                    wait(300);
                    gesture(metricsPill, window, ["--right-click"]);
                    tryCompare(metricsPopup, "visible", true, 3000);
                    verify(metricsPopup.customising);
                    tryCompare(metricsPopup, "revealScale", 1, 3000);
                    const toggle = findChild(metricsPopup.body, "monitorOption_cpuSwitch");
                    gesture(toggle, metricsPopup.nativeWindow, ["--click-only"]);
                    tryVerify(() => !metricsPill.isShown("cpu"), 2000);
                    gesture(toggle, metricsPopup.nativeWindow, ["--click-only"]);
                    tryVerify(() => metricsPill.isShown("cpu"), 2000);
                    metricsPopup.visible = false;
                    wait(300);
                    gesture(metricsPill, window, ["--shift-right-click"]);
                    tryCompare(widgetMenu, "visible", true, 3000);
                    compare(widgetMenu.widgetId, "metrics");
                    widgetMenu.visible = false;
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
