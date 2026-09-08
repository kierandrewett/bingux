import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FileView { id: report; blockWrites: true; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    QtObject {
        id: state
        function expiryProgress(notification) { return 0; }
        function canActivate(entry) { return false; }
        function setPaused(notification, paused) {}
    }
    NotificationState {
        id: refreshState
        property int refreshCount: 0
        function refreshApplicationMetadata() { refreshCount++; }
    }
    DockNotifications {
        id: menu
        width: 320
        height: 400
        state: state
        entries: Array.from({length: 25}, (_, i) => ({notification: {id: i + 1}, appName: "Test", desktopEntry: "", summary: "Notification", body: "Body", appIcon: "", image: "", actions: [], receivedAt: 0, timeoutMs: 0}))
    }
    TestCase {
        when: true
        function test_hidden_menu() {
            wait(200);
            const before = refreshState.refreshCount;
            for (let i = 0; i < 100; i++) DesktopEntries.applications.valuesChanged();
            compare(refreshState.refreshCount, before);
            wait(200);
            compare(refreshState.refreshCount, before + 1);
            compare(menu.renderedNotificationCount, 0);
            menu.menuActive = true;
            wait(100);
            compare(menu.renderedNotificationCount, 25);
            menu.menuActive = false;
            wait(300);
            compare(menu.renderedNotificationCount, 0);
            for (let i = 0; i < 100; i++) menu.entries = menu.entries.slice();
            wait(100);
            compare(menu.renderedNotificationCount, 0);
            report.setText("FAILURES 0\n");
            wait(100);
            finish.start();
        }
    }
}
