from pathlib import Path
import re, shutil, tempfile, subprocess, os
temporary = tempfile.TemporaryDirectory(prefix='bingux-bar-layout-')
root = Path(temporary.name)
source=Path(__file__).resolve().parents[1] / 'shell/bingux'
for p in source.iterdir():
 if p.suffix in ('.qml','.js') or p.name=='qmldir':shutil.copy(p,root/p.name)
s=(source/'shell.qml').read_text()
def block_at(text,start):
 left=text.index('{',start); depth=1;i=left+1
 while depth:
  depth+=(text[i]=='{')-(text[i]=='}');i+=1
 return text[start:i],i
def named(text,name):
 m=re.search(r'\b\w+\s*\{\s*id:\s*'+name+r'\b',text)
 return block_at(text,m.start())[0]
bar=named(s,'topBar');popup=named(s,'barOverflow')
popup=popup.replace('id: barOverflow', 'id: barOverflow; parent: window.contentItem; y: 40; z: 10', 1)
bar=bar.replace('PanelWindow {','Item {',1)
bar=bar.replace('        margins.left: terminalSidebar.leftInset','        x: terminalSidebar.leftInset\n        property int monitorWidth: 1280\n        width: monitorWidth - terminalSidebar.leftInset - terminalSidebar.rightInset\n        height: Theme.barHeight\n        property var screen: window.screen')
bar='\n'.join(line for line in bar.splitlines() if not any(line.strip().startswith(x) for x in ['margins.right:', 'exclusiveZone:', 'WlrLayershell.', 'anchors { top: true; left: true; right: true }']) and line!='        color: "transparent"')
replacements={
 'tray':'Item { id: tray; implicitWidth: 130; implicitHeight: 32 }',
 'privacy':'Item { id: privacy; implicitWidth: 32; implicitHeight: 32 }',
 'metricsPill':'Pill { id: metricsPill; parent: topBar.overflows(metricsPill) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(metricsPill); Layout.row: topBar.controlRow(metricsPill); implicitWidth: 240; visible: profileSettings.metricsEnabled; Text { text: "Metrics" } }',
 'inputSourceSelector':'Pill { id: inputSourceSelector; parent: topBar.overflows(inputSourceSelector) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(inputSourceSelector); Layout.row: topBar.controlRow(inputSourceSelector); visible: metrics.desktopStateAvailable; implicitWidth: 50; property bool menuOpen: false; Text { text: "EN" } }',
 'systemIndicators':'Item { id: systemIndicators; implicitWidth: 100; implicitHeight: 32; property string extraStatusDescription: "" }',
 'notificationCount':'Item { id: notificationCount; implicitWidth: 24; implicitHeight: 24 }'
}
for name,value in replacements.items():bar=bar.replace(named(bar,name),value)
# Expose real production geometry to the test, without production services.
bar=bar.replace('id: clockPill','id: clockPill\n                objectName: "clock"',1).replace('id: rightControls','id: rightControls\n                    objectName: "rightControls"',1)
fixture='''import QtQml.Models
import QtCore
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
 id: root
 property var currentTime: new Date()
 function closePanelsExcept(panel) {}
 function openSearch() {}
 QtObject { id: terminalSidebar; property int leftInset: 0; property int rightInset: 0; property int topInset: 0; property int desktopCornerSize: 0 }
 QtObject { id: profileSettings; property bool metricsEnabled: true }
 QtObject { id: metrics; property bool desktopStateAvailable: true }
 QtObject { id: captureTool; property bool busy: true; property bool recording: true; property string elapsedText: "00:14"; property string state: ""; property int countdown: 0; function stop(){} }
 QtObject { id: calendarPopup; property bool visible: false }
 QtObject { id: controlCentre; property bool visible: false }
 QtObject { id: notificationCentre; property bool visible: false }
 QtObject { id: notificationState; property var allEntries: [1,2,3] }
 FileView { id: results; path: Quickshell.env("BINGUX_BAR_LAYOUT_RESULT") }
 POPUP
 FloatingWindow {
  id: window; implicitWidth: 1280; implicitHeight: 400; visible: true
  BAR
  TestCase {
   when: window.visible
   function verifyOrder() {
    let last = -1;
    for (const item of topBar.barControls) {
     verify(item.x > last, "Stable bar order: " + topBar.orderedControls.indexOf(item));
     last = item.x;
    }
    compare(topBar.barControls[topBar.barControls.length - 1], notificationButton);
   }
   function test_centre_and_overflow() {
    for (const inset of [0, 384]) {
     for (const edge of ["left", "right"]) {
      terminalSidebar.leftInset = edge === "left" ? inset : 0;
      terminalSidebar.rightInset = edge === "right" ? inset : 0;
      wait(100);
      compare(clockPill.mapToItem(topBar, clockPill.width/2,0).x, topBar.width/2);
      verify(topBar.overflowItems.length > 0);
      verify(rightControls.width <= topBar.controlsBudget + 1, "Controls fit reserved half");
      mouseClick(overflowButton);
      wait(100);
      for (const item of topBar.overflowItems) compare(item.parent, overflowColumn);
      verify(overflowColumn.implicitHeight > 0);
      verify(!topBar.overflows(notificationButton));
      verify(!topBar.overflows(systemPill));
      verify(!topBar.overflows(inputSourceSelector));
      verifyOrder();
      barOverflow.visible = false;
     }
    }
    barOverflow.visible = true;
    wait(100);
    verify(topBar.overflowItems.length >= 2);
    const moved = topBar.overflowItems[topBar.overflowItems.length - 1];
    const first = topBar.overflowItems[0];
    const movedHandle = findChild(moved, "barReorderHandle");
    mousePress(movedHandle, movedHandle.width / 2, movedHandle.height / 2, Qt.LeftButton, Qt.ControlModifier);
    const destination = first.mapToItem(movedHandle, first.width / 2, 1);
    mouseMove(movedHandle, destination.x, destination.y, 30);
    verify(movedHandle.dragging);
    mouseRelease(movedHandle, destination.x, destination.y, Qt.LeftButton, Qt.ControlModifier);
    wait(230);
    compare(topBar.overflowItems[0], moved, "Overflow items can be reordered");
    barOverflow.visible = false;
    topBar.monitorWidth = 3000;
    terminalSidebar.leftInset = 0; terminalSidebar.rightInset = 0;
    wait(100);
    compare(topBar.overflowItems.length, 0);
    verifyOrder();
    const handle = findChild(notificationButton, "barReorderHandle");
    verify(handle !== null);
    const start = handle.mapToItem(window.contentItem, handle.width / 2, handle.height / 2);
    const end = inputSourceSelector.mapToItem(window.contentItem, 1, inputSourceSelector.height / 2);
    const neighbourStart = inputSourceSelector.mapToItem(window.contentItem, 0, 0).x;
    const savedBeforeDrag = barPreferences.order;
    mousePress(window.contentItem, start.x, start.y, Qt.LeftButton, Qt.ControlModifier);
    mouseMove(window.contentItem, end.x, end.y, 30);
    verify(handle.dragging, "Ctrl-drag starts reorder");
    compare(barPreferences.order, savedBeforeDrag, "Preview does not save prematurely");
    const draggedCentre = notificationButton.mapToItem(window.contentItem, notificationButton.width / 2, notificationButton.height / 2);
    verify(Math.abs(draggedCentre.x - end.x) < 1, "Real control follows pointer");
    wait(60);
    const neighbourDuring = inputSourceSelector.mapToItem(window.contentItem, 0, 0).x;
    verify(neighbourDuring > neighbourStart, "Neighbour slides aside during drag");
    verify(neighbourDuring < neighbourStart + topBar.reorderShift(inputSourceSelector), "Displacement is animated");
    mouseRelease(window.contentItem, end.x, end.y, Qt.LeftButton, Qt.ControlModifier);
    verify(topBar.settlingReorder, "Release eases into the destination");
    compare(barPreferences.order, savedBeforeDrag, "Save waits for settle");
    wait(230);
    verify(notificationButton.x < inputSourceSelector.x, "Notification count moves before keyboard");
    verify(barPreferences.order.indexOf("notifications") < barPreferences.order.indexOf("keyboard"));
    const landed = notificationButton.mapToItem(window.contentItem, 0, 0).x;
    wait(40);
    compare(notificationButton.mapToItem(window.contentItem, 0, 0).x, landed, "No jump after commit");
    // A cancelled drag eases back without changing the saved order.
    const savedAfterDrag = barPreferences.order;
    const cancelStart = handle.mapToItem(window.contentItem, handle.width / 2, handle.height / 2);
    mousePress(window.contentItem, cancelStart.x, cancelStart.y, Qt.LeftButton, Qt.ControlModifier);
    mouseMove(window.contentItem, cancelStart.x - 80, cancelStart.y, 30);
    mouseMove(window.contentItem, cancelStart.x - 80, 100, 30);
    mouseRelease(window.contentItem, cancelStart.x - 80, 100, Qt.LeftButton, Qt.ControlModifier);
    wait(230);
    compare(barPreferences.order, savedAfterDrag);
    compare(notificationButton.mapToItem(window.contentItem, 0, 0).x, landed);
    // A normal click still reaches the underlying button.
    mouseClick(notificationButton);
    compare(notificationCentre.visible, true);
    // Repeat overflow and return with the custom order.
    topBar.monitorWidth = 1280;
    wait(100);
    topBar.monitorWidth = 3000;
    wait(100);
    verify(notificationButton.x < inputSourceSelector.x, "Custom order survives overflow");
    compare(clockPill.mapToItem(topBar, clockPill.width/2,0).x, topBar.width/2);
   }
   function cleanupTestCase() { results.setText("FAILURES " + qtest_results.failCount); Qt.quit(); }
  }
 }
}'''.replace('\n POPUP\n','\n'+popup+'\n').replace('\n  BAR\n','\n'+bar+'\n')
(root/'shell.qml').write_text(fixture)

# Replace only platform surfaces; layout and overflow code above comes from shell.qml.
(root / 'ShellPopup.qml').write_text('import QtQuick\nItem {\n id: root\n default property alias contents: body.data\n property var screen\n property int popupWidth: 200\n property int popupHeight: 100\n property int contentPadding: 8\n property real preferredX: 0\n property real preferredY: 0\n property alias contentItem: body\n visible: false\n width: popupWidth\n height: popupHeight\n Item { id: body; x: root.contentPadding; y: root.contentPadding; width: root.width-root.contentPadding*2 }\n}\n')
(root / 'BarTooltip.qml').write_text('import QtQuick\nQtObject {\n property var anchorItem\n property var barWindow\n property bool requested: false\n property string text\n property bool reorderable: false\n}\n')
(root / 'config' / 'bingux').mkdir(parents=True)
result = root / 'result'
process = subprocess.run(['dbus-run-session', '--', os.environ.get('QUICKSHELL_BIN', 'qs'), '-p', str(root)], env=dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), QT_QPA_PLATFORM='offscreen', BINGUX_BAR_LAYOUT_RESULT=str(result)), capture_output=True, text=True, timeout=15)
report = result.read_text() if result.exists() else 'FAIL: no result'
print(report)
if report.strip() == 'FAILURES 0':
 saved = (root / 'config' / 'bingux' / 'top-bar.ini').read_text()
 assert saved.index('notifications') < saved.index('keyboard'), saved
assert process.returncode == 0 and report.strip() == 'FAILURES 0', process.stdout + process.stderr
assert not re.search(r'TypeError:|ReferenceError:|Binding loop|Cannot stack', process.stdout + process.stderr), process.stdout + process.stderr
# Start a new process against the same preference file to prove restart restoration.
start = fixture.index('  TestCase {')
fixture = fixture[:start] + '''  TestCase {
   when: window.visible
   function test_restored_order() {
    topBar.monitorWidth = 3000;
    wait(100);
    verify(notificationButton.x < inputSourceSelector.x);
    compare(topBar.orderedControls.length, topBar.defaultControls.length);
   }
   function cleanupTestCase() { results.setText("FAILURES " + qtest_results.failCount); Qt.quit(); }
  }
 }
}'''
(root / 'shell.qml').write_text(fixture)
result.unlink()
restarted = subprocess.run(['dbus-run-session', '--', os.environ.get('QUICKSHELL_BIN', 'qs'), '-p', str(root)], env=dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), QT_QPA_PLATFORM='offscreen', BINGUX_BAR_LAYOUT_RESULT=str(result)), capture_output=True, text=True, timeout=15)
assert restarted.returncode == 0 and result.read_text().strip() == 'FAILURES 0', restarted.stdout + restarted.stderr
print('PASS: order restored in a new process')
temporary.cleanup()
