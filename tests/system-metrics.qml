import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    property int performanceClicks: 0
    property int configureClicks: 0
    QtObject {
        id: sample
        property bool available: true
        property var sampleHardware: ({
            gpus: [], storage: [], processCount: 80,
            processes: Array.from({length: 80}, (_, index) => ({
                pid: 900000 + index,
                name: (index % 2 ? "Worker " : "Sample ") + index,
                executable: index % 2 ? "worker" : "sample",
                cpuPercent: index / 2, memoryBytes: 1048576 * (index + 1), state: "S"
            }))
        })
        property var latest: ({cpuPercent: 24, memoryUsedBytes: 12.4 * 1073741824, memoryTotalBytes: 32 * 1073741824, networkReceiveBytesPerSecond: 240000, networkTransmitBytesPerSecond: 32000, extra: {hardware: sample.sampleHardware, cpuCores: Array.from({length: 16}, (_, id) => ({id, usage: id * 5})), cpuTemperatureCelsius: 54.2, load1: 2.35, logicalCpus: 16, swapUsedBytes: 1073741824, swapTotalBytes: 8589934592, diskReadBytesPerSecond: 24000000, diskWriteBytesPerSecond: 1200000}})
        property var history: []
        readonly property string cpuLabel: "CPU " + latest.cpuPercent + "%"
        function formatRate(rate) { return Math.round(rate / 1024) + "K/s"; }
        function formatBytes(bytes) { return (bytes / 1073741824).toFixed(1) + "G"; }
    }
    RollingNumber { id: rollProbe; text: "9"; value: 9; visible: false }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: results; path: Quickshell.env("BINGUX_METRICS_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 560
        implicitHeight: 960
        color: Theme.barBackground
        SystemMetrics {
            id: widget
            anchors.horizontalCenter: parent.horizontalCenter
            y: 4
            systemMetrics: sample
            preferencesLocation: Qt.resolvedUrl("monitors.ini")
            onConfigureRequested: { configureClicks++; popup.showPage(true); }
            onPerformanceRequested: { performanceClicks++; popup.showPage(false); }
        }
        SystemMetricsPopup { id: popup; hostItem: window.contentItem; monitorWidget: widget }
        TestCase {
            when: window.visible
            name: "SystemMetrics"
            property string checks: ""
            function check(value, message) { checks += "CHECK " + value + " " + (message || "") + "\n"; results.setText(checks); verify(value, message); }
            function equal(actual, expected, message) { checks += "EQUAL " + actual + " / " + expected + " " + (message || "") + "\n"; results.setText(checks); compare(actual, expected, message); }
            function test_interactions_and_history() {
                results.setText("FAIL: metrics checks did not complete\n");
                if (Quickshell.env("BINGUX_HARDWARE_RECORD")) {
                    sample.latest = Object.assign({}, sample.latest, {extra: Object.assign({}, sample.latest.extra, {hardware: JSON.parse(Quickshell.env("BINGUX_HARDWARE_RECORD")).extra.hardware})});
                }
                const now = Date.now();
                const points = [];
                for (let n = 0; n < 300; n++) points.push({at: now - (299 - n) * 1000, cpu: 26 + Math.sin(n / 8) * 18 + Math.pow(Math.sin(n / 3), 6) * 35, memory: 41 + Math.sin(n / 40) * 4, memoryUsed: 12.4 * 1073741824, memoryTotal: 32 * 1073741824, receive: 12000 + Math.pow(Math.sin(n / 16), 4) * 440000, send: 8000 + Math.pow(Math.sin(n / 9), 4) * 92000, temperature: 54 + Math.sin(n / 12) * 4, load: 2.35 + Math.sin(n / 8), swap: 12.5, swapUsed: 1073741824, swapTotal: 8589934592, diskRead: 12000 + Math.pow(Math.sin(n / 9), 4) * 24000000, diskWrite: 8000 + Math.pow(Math.sin(n / 8), 4) * 1200000});
                for (let n = 0; n < points.length; n++) for (let core = 0; core < 16; core++)
                    points[n]["cpu" + core] = 20 + core * 2 + Math.sin(n / (4 + core) + core) * 18;
                sample.history = points;
                wait(100);
                const cpuReadout = findChild(widget, "cpuReadout");
                const memoryReadout = findChild(widget, "memoryReadout");
                equal(cpuReadout.x, memoryReadout.x, "CPU and memory share a column");
                check(memoryReadout.y >= cpuReadout.y + cpuReadout.height, "two rows do not overlap");
                check(widget.implicitWidth < 180, "default monitors stay compact with graphs");
                const barGraph = findChild(widget, "cpuBarGraph");
                check(barGraph.width >= 40 && barGraph.points.length > 50, "mini graph displays measured history");
                check(memoryReadout.mapToItem(widget, 0, memoryReadout.height).y <= widget.height, "rows fit the top bar");
                rollProbe.value = 10; rollProbe.text = "10";
                wait(30);
                check(Theme.reducedMotion ? !rollProbe.animating : rollProbe.animating, "rolling numbers respect motion preference");
                equal(rollProbe.direction, 1);
                wait(180);
                equal(rollProbe.displayedText, "10");
                rollProbe.value = 8; rollProbe.text = "8";
                wait(30);
                if (!Theme.reducedMotion) equal(rollProbe.direction, -1);
                rollProbe.value = 42; rollProbe.text = "42";
                rollProbe.value = 73; rollProbe.text = "73";
                wait(350);
                equal(rollProbe.displayedText, "73", "rapid scrubbing coalesces to latest value");
                wait(300);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) widget.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".default-bar.png"));
                wait(100);
                mouseClick(widget, widget.width / 2, widget.height / 2, Qt.LeftButton);
                wait(250);
                check(popup.visible && !popup.customising, "left click opens performance");
                check(popup.popupWidth <= 520 && popup.popupHeight <= 640 && popup.popupHeight <= window.height * 0.65,
                    "performance popup stays compact");
                const scroll = findChild(popup.contentItem, "monitorPageScroll").contentItem;
                equal(scroll.boundsBehavior, Flickable.StopAtBounds, "monitor scrolling cannot overshoot");
                equal(scroll.boundsMovement, Flickable.StopAtBounds, "monitor scrolling has no rubber-band movement");
                scroll.contentY = scroll.originY;
                scroll.flick(0, 2000);
                wait(100);
                equal(scroll.contentY, scroll.originY, "flicking above the first reading stays at the top");
                const bottom = Math.max(scroll.originY, scroll.contentHeight - scroll.height);
                scroll.contentY = bottom;
                scroll.flick(0, -2000);
                wait(100);
                equal(scroll.contentY, bottom, "flicking below the last reading stays at the bottom");
                scroll.cancelFlick();
                scroll.contentY = scroll.originY;
                equal(performanceClicks, 1);
                equal(configureClicks, 0);
                const graph = findChild(popup.contentItem, "logicalCpuGraph_0");
                check(graph.points.length >= 55 && graph.points.length <= 61, "one-minute view uses measured history");
                mouseMove(graph, graph.width * 0.55, graph.height / 2);
                wait(50);
                check(graph.inspectedPoint !== null, "hover inspects a historical sample");
                wait(320);
                mouseClick(findChild(popup.contentItem, "monitorRange_5m"));
                wait(100);
                check(graph.points.length > 290, "five-minute range includes older samples");
                mouseClick(findChild(popup.contentItem, "monitorRange_1m"));
                wait(100);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".performance.png"));
                wait(100);
                wait(200);
                for (let core = 0; core < 16; core++) {
                    const coreGraph = findChild(popup.contentItem, "logicalCpuGraph_" + core);
                    verify(coreGraph !== null && coreGraph.visible);
                    compare(coreGraph.maximum, 100);
                    verify(coreGraph.points.length > 50);
                    compare(coreGraph.metric, "cpu" + core);
                }
                mouseClick(findChild(popup.contentItem, "monitorRange_5m"));
                wait(100);
                verify(findChild(popup.contentItem, "logicalCpuGraph_0").points.length > 290);
                mouseClick(findChild(popup.contentItem, "monitorRange_1m"));
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".cores.png"));
                wait(100);
                wait(300);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".hardware.png"));
                wait(100);
                mouseClick(findChild(popup.contentItem, "performancePage_processes"));
                wait(200);
                const details = findChild(popup.contentItem, "hardwareDetails");
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".processes.png"));
                if (DesktopEntries.byId("org.gnome.Ptyxis"))
                    compare(details.applicationFor({name: "ptyxis", executable: "ptyxis"}).id, "org.gnome.Ptyxis");
                if (DesktopEntries.byId("discord-canary"))
                    compare(details.applicationFor({name: "DiscordCanary", executable: "DiscordCanary"}).id, "discord-canary");
                const probePid = Number(Quickshell.env("BINGUX_PROCESS_PROBE"));
                if (probePid) {
                    const table = findChild(details, "processTable");
                    const probe2 = Number(Quickshell.env("BINGUX_PROCESS_PROBE_2"));
                    if (probe2) {
                        const first = findChild(details, "processRow_" + probePid);
                        const second = findChild(details, "processRow_" + probe2);
                        mouseClick(first, 50, 15);
                        mouseClick(second, 50, 15, Qt.LeftButton, Qt.ControlModifier);
                        compare(table.selectedRecords.length, 2);
                    }
                    for (const action of ["pause", "resume", "end"]) {
                        const row = findChild(details, "processRow_" + probePid);
                        verify(row !== null);
                        mouseClick(row, row.width / 2, row.height / 2, Qt.RightButton);
                        wait(200);
                        verify(findChild(details, "processContextMenu").visible);
                        keyClick(Qt.Key_Down);
                        const button = findChild(details, "processAction_" + action);
                        verify(button.enabled);
                        mouseClick(button);
                        tryVerify(() => details.actionMessage.length > 0, 4000);
                        compare(details.actionMessage, probe2 ? ({pause: "Paused 2 processes", resume: "Resumed 2 processes", end: "End requested for 2 processes"})[action] : ({pause: "Process paused", resume: "Process resumed", end: "End requested"})[action]);
                    }
                }
                const table = findChild(details, "processTable");
                mouseClick(findChild(details, "processSort_Memory"));
                compare(table.sortKey, "Memory");
                verify(table.descending);
                mouseClick(findChild(details, "processSort_Memory"));
                verify(!table.descending);
                const search = findChild(details, "processSearch");
                mouseClick(search);
                keyClick(Qt.Key_P);
                compare(table.search, "p");
                verify(table.sortedRecords.every(process => (process.name + process.executable + process.pid).toLowerCase().includes("p")));
                keyClick(Qt.Key_Escape);
                compare(table.search, "");
                const rows = findChild(details, "processRows");
                tryCompare(rows, "count", table.sortedRecords.length);
                tryCompare(rows, "contentHeight", rows.count * table.rowHeight);
                rows.contentY = 0;
                mouseWheel(rows, 80, 60, 0, -120);
                compare(rows.contentY, 136);
                wait(250);
                compare(rows.contentY, 136); // No momentum after the wheel event.
                mouseWheel(rows, 80, 60, 0, 120);
                compare(rows.contentY, 0);
                rows.contentY = 300;
                const before = rows.contentY;
                sample.latest = Object.assign({}, sample.latest, {extra: Object.assign({}, sample.latest.extra, {hardware: Object.assign({}, sample.latest.extra.hardware, {processes: sample.latest.extra.hardware.processes.map(process => Object.assign({}, process))})})});
                wait(100);
                compare(rows.contentY, before);
                const tabButton = findChild(popup.contentItem, "performancePage_usage");
                const tabY = tabButton.mapToItem(popup.body, 0, 0).y;
                mouseClick(findChild(popup.contentItem, "performancePage_services"));
                wait(500);
                const services = findChild(popup.contentItem, "servicesPanel");
                verify(services.visible);
                services.scope = "System";
                tryVerify(() => services.services.length > 0 || services.error.length > 0, 4000);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".services.png"));
                wait(250); // Let the asynchronous Services capture finish before switching pages.
                const serviceTable = findChild(services, "servicesTable");
                compare(serviceTable.mode, "services");
                const states = serviceTable.stateGroups.slice();
                for (const state of states.concat(states.slice(0, 1))) {
                    mouseClick(findChild(serviceTable, "processSort_State"));
                    equal(serviceTable.recordAt(0).state, state, "State group cycle");
                }
                if (services.services.length > 30) {
                    const serviceRows = findChild(serviceTable, "processRows");
                    serviceRows.positionViewAtBeginning();
                    mouseWheel(serviceRows, 80, 60, 0, -120);
                    equal(serviceRows.contentY - serviceRows.originY, 4 * serviceTable.rowHeight, "Fixed wheel steps after State sorting");
                    wait(250);
                    equal(serviceRows.contentY - serviceRows.originY, 4 * serviceTable.rowHeight, "Fixed wheel steps after State sorting");
                }
                const historyToggle = findChild(popup.contentItem, "performancePage_usage");
                mouseClick(historyToggle);
                wait(100);
                const scrolling = findChild(popup.contentItem, "monitorPageScroll").contentItem;
                scrolling.contentY = 300;
                wait(100);
                compare(tabButton.mapToItem(popup.body, 0, 0).y, tabY);
                verify(scrolling.contentY > 0);
                scrolling.contentY = 0;
                equal(widget.valueFor("temperature"), "54°C");
                equal(widget.valueFor("load"), "2.35");
                equal(widget.valueFor("swap"), "1.0G");
                const swap = findChild(popup.contentItem, "swapHistoryGraph");
                verify(swap.visible && swap.points.length > 50);
                compare(swap.maximum, 100);
                check(findChild(popup.contentItem, "diskHistory").visible, "system page has disk read/write history");
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".system.png"));
                wait(100);
                popup.visible = false;
                wait(200);
                mouseClick(widget, widget.width / 2, widget.height / 2, Qt.RightButton);
                wait(200);
                check(popup.visible && popup.customising, "right click opens customisation");
                equal(performanceClicks, 1);
                equal(configureClicks, 1);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) popup.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".chooser.png"));
                wait(100);
                mouseClick(findChild(popup.contentItem, "monitorOption_receiveSwitch"));
                mouseClick(findChild(popup.contentItem, "monitorOption_sendSwitch"));
                check(widget.isShown("receive") && widget.isShown("send"));
                mouseClick(findChild(popup.contentItem, "monitorOption_temperatureSwitch"));
                check(widget.isShown("temperature"), "new monitors are opt-in");
                widget.setShown("temperature", false);
                wait(200);
                if (Quickshell.env("BINGUX_METRICS_SCREENSHOT")) widget.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_METRICS_SCREENSHOT") + ".bar.png"));
                wait(100);
                popup.showPage(false);
                check(popup.visible && !popup.customising, "switching page keeps popup open");
                popup.visible = false;
                wait(200);
                widget.forceActiveFocus();
                keyClick(Qt.Key_Return);
                wait(200);
                check(popup.visible && !popup.customising, "Enter opens performance");
                popup.visible = false;
                wait(200);
                widget.forceActiveFocus();
                keyClick(Qt.Key_F10, Qt.ShiftModifier);
                wait(200);
                check(popup.visible && popup.customising, "Shift+F10 opens customisation");
                popup.visible = false;
                wait(200);
                const width = widget.implicitWidth;
                sample.available = false;
                wait(100);
                equal(widget.implicitWidth, width);
                equal(widget.valueFor("cpu"), "—");
                equal(sample.history.length, 300, "outage preserves measured history");
                widget.setShown("cpu", false);
                widget.setShown("memory", false);
                widget.setShown("receive", false);
                widget.setShown("send", false);
                equal(widget.selectedNames.length, 1);
                results.setText("PASS: rolling digits and rapid scrubbing; temperature, load, swap and disk metrics; mouse/keyboard routing, history, customisation and outage states\n");
                finish.start();
            }
        }
    }
}
