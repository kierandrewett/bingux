import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
 FileView { id: result; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
 FloatingWindow {
  id: window; visible: true; implicitWidth: 320; implicitHeight: 760
  color: Theme.barBackground
  QtObject {
   id: metrics
   property bool available: true
   property var latest: ({cpuPercent: 36, memoryUsedBytes: 22000000000, memoryTotalBytes: 64000000000, networkReceiveBytesPerSecond: 750000, networkTransmitBytesPerSecond: 54000, extra: {cpuCores: Array.from({length: 16}, (_, id) => ({id, usage: id * 6})), hardware: {
    cpuModel: "AMD Ryzen test processor with a long model name", cpuMhz: 4100, uptimeSeconds: 99000, kernel: "6.15.0-test", processCount: 200, runningProcesses: 2, threads: 1400,
    gpus: [{name: "AMD Radeon graphics with a long model name", driver: "amdgpu", pciAddress: "0000:2d:00.0", busyPercent: 32, memoryUsedBytes: 4000000000, memoryTotalBytes: 16000000000, temperatureCelsius: 48, powerWatts: 55.4, clockMhz: 1500, fanRpm: null}],
    storage: [{path: "/", totalBytes: 1000000000000, availableBytes: 300000000000}],
    processes: [{pid: 1, name: "memory-heavy-process", cpuPercent: 2, memoryBytes: 2000000000, threads: 4}, {pid: 2, name: "cpu-heavy-process", cpuPercent: 155, memoryBytes: 200000000, threads: 8}]
   }}})
   property var history: Array.from({length: 61}, (_, i) => ({at: Date.now() - (60-i)*1000, cpu: 20 + Math.sin(i*.5)*15 + i*.3, memory: 34+i*.01, memoryUsed: 22000000000, memoryTotal: 64000000000, receive: 650000+Math.sin(i*.35)*200000, send: 54000+Math.cos(i*.3)*30000}))
   function formatBytes(value) { return value > 1e9 ? (value/1e9).toFixed(1)+"G" : Math.round(value/1000)+"K"; }
   function formatRate(value) { return formatBytes(value)+"/s"; }
  }
  SidebarMonitor { id: monitor; x: 8; y: 8; width: 304; height: 744; metrics: metrics }
  TestCase {
   when: window.visible
   function test_live_contract() {
    const record = Quickshell.env("BINGUX_HARDWARE_RECORD");
    if (!record) return;
    const component = Qt.createComponent("Metrics.qml");
    compare(component.status, Component.Ready);
    const validator = component.createObject(window.contentItem);
    verify(validator.isMetricsRecord(JSON.parse(record)), "Live hardware record must pass shell validation");
    const invalid = JSON.parse(record);
    invalid.extra.hardware.processes[0].cpuPercent = "invalid";
    verify(!validator.isMetricsRecord(invalid));
    validator.destroy();
   }
   function test_state_sort_cycle() {
    const component = Qt.createComponent("ProcessTable.qml");
    compare(component.status, Component.Ready);
    const table = component.createObject(window.contentItem, {
     width: 300, height: 300, mode: "services", sortKey: "Name", descending: false,
     applicationFor: () => null, formatBytes: () => "",
     records: [
      {key: "a", name: "a.service", state: "running"},
      {key: "b", name: "b.service", state: "dead"},
      {key: "c", name: "c.service", state: "failed"},
      {key: "d", name: "d.service", state: "exited"},
      {key: "e", name: "e.service", state: "running"}
     ]
    });
    verify(table !== null);
    const firstStates = [];
    wait(50);
    table.selectIndex(0); // Explicit sorting must work while selection holds row order.
    for (let i = 0; i < 5; i++) {
     mouseClick(findChild(table, "processSort_State"));
     firstStates.push(table.recordAt(0).state);
     const states = Array.from({length: 5}, (_, index) => table.recordAt(index).state);
     compare(states.lastIndexOf("running") - states.indexOf("running"), 1, "State groups must stay together");
    }
    compare(firstStates.join(","), "dead,exited,failed,running,dead");
    compare(table.selectedRecords.length, 1);
    table.search = "a.service";
    table.selectSort("State");
    compare(table.recordAt(0).state, "running");
    table.records = [];
    table.selectSort("State");
    compare(table.sortedRecords.length, 0);
    table.destroy();
   }
   function test_layout() {
    const panel = findChild(monitor, "sidebarPerformance");
    for (const width of [150, 320, 384]) {
     monitor.width = width - 16;
     wait(100);
     verify(panel.width <= monitor.width);
     function check(item) {
      for (const child of item.children) {
       if (!child.visible) continue;
       if (child.objectName === "processTableViewport") continue;
       if (child.width > item.width + 1) console.warn("OVERFLOW", child, child.width, item, item.width);
       verify(child.width <= item.width + 1, "Child must fit parent: " + child);
       check(child);
      }
     }
     check(panel);
     for (const tab of ["usage", "services", "processes"]) {
      panel.page = tab; wait(100); check(panel);
      if (tab === "processes") {
       compare(panel.height, monitor.height);
       verify(findChild(panel, "processTableViewport").height > monitor.height - 180);
      }
     }
     panel.page = "usage";
    }
    panel.page = "processes";
    wait(50);
    const details = findChild(panel, "hardwareDetails");
    compare(details.processes[0].pid, 2);
    details.processSort = "Memory";
    compare(details.processes[0].pid, 1);
    const table = findChild(panel, "processTable");
    table.selectSort("Memory");
    compare(details.processes[0].pid, 2);
    table.selectSort("PID");
    compare(details.processes[0].pid, 1);
    table.search = "cpu-heavy";
    compare(details.processes.length, 1);
    compare(details.processes[0].pid, 2);
    table.search = "";
    table.selectIndex(0);
    table.selectIndex(1, Qt.ControlModifier);
    compare(table.selectedRecords.length, 2);
    table.selectIndex(0);
    table.selectIndex(1, Qt.ShiftModifier);
    compare(table.selectedRecords.length, 2);
    table.search = "cpu-heavy";
    table.selectAll();
    compare(table.selectedRecords.length, 1);
    table.search = "";
    metrics.available = false;
    compare(details.processes.length, 0);
    metrics.available = true;
    panel.range = "5m";
    compare(panel.duration, 300000);
    const component = Qt.createComponent("SidebarMonitor.qml");
    compare(component.status, Component.Ready);
    const restored = component.createObject(window.contentItem, {width: 300, height: 740, metrics: metrics});
    verify(restored !== null);
    compare(findChild(restored, "sidebarPerformance").range, "5m");
    restored.destroy();
    metrics.available = false;
    wait(50);
    metrics.available = true;
    monitor.width = 304;
    wait(400);
    panel.page = "usage";
    wait(100);
    verify(panel.implicitHeight > monitor.height, "Overview content must scroll");
    const tabs = findChild(panel, "performancePage_usage");
    const tabY = tabs.mapToItem(monitor, 0, 0).y;
    const scrolling = findChild(panel, "monitorPageScroll").contentItem;
    scrolling.contentY = 250;
    wait(100);
    compare(tabs.mapToItem(monitor, 0, 0).y, tabY);
    scrolling.contentY = 0;
    verify(findChild(panel, "swapHistoryGraph") !== null);
    if (Quickshell.env("BINGUX_SYSTEM_SCREENSHOT")) { panel.page = "processes"; wait(300); grabImage(monitor).save(Quickshell.env("BINGUX_SYSTEM_SCREENSHOT")); }
   }
   function cleanupTestCase() { result.setText("FAILURES " + qtest_results.failCount + "\n"); Qt.quit(); }
  }
 }
}
