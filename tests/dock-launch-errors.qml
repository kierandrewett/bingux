import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    QtObject { id: lateWindow; property string title: "Late window"; property string appId: "bingux-missing-launch-test"; property bool activated: false; property bool minimized: false }
    Dock { id: dock; visible: false; settings: ({pinnedApps: []}) }
    TestCase {
        when: true
        function cleanupTestCase() { finish.start(); }
        function test_failures() {
            wait(300);
            const id = "bingux-missing-launch-test";
            const group = {id: id, desktopEntry: {id: id, name: "Launch test"}, windows: []};
            dock.appGroups = [group];
            dock.visible = true;
            dock.launch(group, false);
            tryVerify(() => !!dock.launchFailures[id], 10000);
            const dialog = findChild(dock, "launchErrorDialog");
            verify(dialog !== null);
            verify(dialog.visible);
            verify(dialog.message.includes("could not be found"));
            verify(!dock.launchAttempts[id]);
            const icon = findChild(dock.contentItem, "dockApplicationIcon");
            verify(icon !== null);
            compare(icon.opacity, 0.5);
            verify(icon.layer.enabled);
            let captured = false;
            dialog.contentItem.children[0].grabToImage(result => {
                result.saveToFile("/tmp/bingux-launch-error.png");
                captured = true;
            });
            tryVerify(() => captured, 2000);
            dialog.dismissCurrent();
            verify(!dialog.visible);
            verify(!!dock.launchFailures[id]);
            dock.launchAttempts = {first: {serial: 100, windows: [], deadline: Date.now() + 5000}, second: {serial: 101, windows: [], deadline: Date.now() + 5000}};
            dock.failLaunch("first", 99, "Stale error");
            verify(!dock.launchFailures.first);
            dock.failLaunch("first", 100, "First failure");
            verify(!!dock.launchAttempts.second);
            dock.failLaunch("second", 101, "Second failure");
            compare(dialog.applicationId, "first");
            dialog.dismissCurrent();
            compare(dialog.applicationId, "second");
            dialog.dismissCurrent();
            dock.appGroups = [{id: id, desktopEntry: group.desktopEntry, windows: [lateWindow]}];
            tryVerify(() => !dock.launchFailures[id], 1000);
            compare(icon.opacity, 1);
            dock.launchAttempts = {timeout: {serial: 200, windows: [], deadline: Date.now() - 1}};
            tryVerify(() => !!dock.launchFailures.timeout, 1000);
            verify(dialog.visible);
            verify(dialog.message.includes("may still be starting"));
            dialog.dismissCurrent();
            dock.launchFailures = Object.assign({}, dock.launchFailures, {
                timeout: Object.assign({}, dock.launchFailures.timeout, {expiresAt: Date.now() - 1})
            });
            tryVerify(() => !dock.launchFailures.timeout, 1000);
            verify(!dialog.visible);
            report.setText("FAILURES 0\nMissing entry, visible error, grey icon, late recovery, timeout and concurrent launches verified\n");
            finish.start();
        }
    }
}
