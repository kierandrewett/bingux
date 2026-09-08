import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Dock { id: dock; visible: false; settings: ({pinnedApps: []}) }
    TestCase {
        when: true
        property var original: []
        function test_launch() {
            wait(300);
            original = ToplevelManager.toplevels.values.slice();
            const app = DesktopEntries.byId("org.gnome.Nautilus");
            verify(app !== null);
            const group = {id: app.startupClass || app.id.replace(/\.desktop$/, ""), desktopEntry: app};
            report.setText("FAIL: launch not completed\n");
            const started = Date.now();
            dock.launch(group, true);
            tryVerify(() => ToplevelManager.toplevels.values.some(w => !original.includes(w) && w.appId.includes("Nautilus")), 6000);
            report.setText("FAIL: Nautilus window appeared but pill still pending: " + dock.pendingLaunchGroupId + "\n");
            tryCompare(dock, "pendingLaunchGroupId", "", 1500);
            report.setText("FAILURES 0\nlaunch completed in " + (Date.now() - started) + "ms\n");
        }
        function cleanupTestCase() {
            for (const w of ToplevelManager.toplevels.values)
                if (!original.includes(w) && w.appId.includes("Nautilus")) w.close();
            finish.start();
        }
    }
}
