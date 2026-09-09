import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Dock { id: dock; settings: ({pinnedApps: []}) }
    TestCase {
        when: true
        property var original: []
        function test_close_multiple() {
            wait(300);
            original = ToplevelManager.toplevels.values.slice();
            const app = DesktopEntries.byId("org.gnome.Nautilus");
            const group = {id: app.startupClass || app.id.replace(/\.desktop$/, ""), desktopEntry: app, windows: []};
            function created() { return ToplevelManager.toplevels.values.filter(w => !original.includes(w) && w.appId.includes("Nautilus")); }
            dock.launch(group, true);
            tryVerify(() => created().length === 1, 6000);
            dock.launch(group, true);
            tryVerify(() => created().length === 2, 6000);
            wait(400);
            let button = null;
            for (let i = 0; i < dock.testItems.count; i++) {
                const item = dock.testItems.itemAt(i);
                if (item.currentGroup.windows.includes(created()[0])) button = item;
            }
            verify(button !== null);
            button.menuOpen = true;
            wait(300);
            for (let remaining = 2; remaining > 0; remaining--) {
                const window = created()[0];
                let action = null;
                for (const child of button.testMenuEntries.children) {
                    if (child.modelData === window) action = child;
                }
                verify(action !== null);
                const close = findChild(action, "closeWindowButton");
                verify(close !== null);
                mouseClick(close);
                verify(action.closing, "Close starts the spinner");
                tryVerify(() => created().length === remaining - 1, 3000);
                if (remaining > 1) verify(button.menuOpen, "Menu remains open after closing a window");
                wait(400);
            }
            report.setText("FAILURES 0\nTwo real windows closed through menu buttons; spinner state and retained menu verified\n");
        }
        function cleanupTestCase() {
            for (const window of ToplevelManager.toplevels.values)
                if (!original.includes(window) && window.appId.includes("Nautilus")) window.close();
            finish.start();
        }
    }
}
