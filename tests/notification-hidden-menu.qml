import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    Timer {
        id: finish
        interval: 200
        onTriggered: Qt.quit()
    }
    FileView {
        id: report
        blockWrites: true
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }
    QtObject {
        id: state
        function expiryProgress(notification) {
            return 0;
        }
        function canActivate(entry) {
            return false;
        }
        function setPaused(notification, paused) {
        }
    }
    NotificationState {
        id: refreshState
        property int refreshCount: 0
        function refreshApplicationMetadata() {
            refreshCount++;
        }
    }
    DockNotifications {
        id: menu
        width: 320
        height: 400
        state: state
        entries: Array.from({
            length: 25
        }, (_, i) => ({
                    notification: {
                        id: i + 1
                    },
                    appName: "Test",
                    desktopEntry: "",
                    summary: "Notification",
                    body: "Body",
                    appIcon: "",
                    image: "",
                    actions: [],
                    receivedAt: 0,
                    timeoutMs: 0
                }))
    }
    TestCase {
        when: true
        function test_hidden_menu() {
            wait(200);
            const before = refreshState.refreshCount;
            const catalogBefore = ApplicationCatalog.entries;
            for (let i = 0; i < 100; i++)
                DesktopEntries.applications.valuesChanged();
            compare(refreshState.refreshCount, before);
            verify(ApplicationCatalog.entries === catalogBefore);
            DesktopEntries.applicationsChanged();
            wait(200);
            compare(refreshState.refreshCount, before + 1);
            verify(ApplicationCatalog.entries !== catalogBefore);
            compare(menu.renderedNotificationCount, 0);
            compare(menu.animationsEnabled, false);
            compare(menu.smoothScrolling, !Theme.reducedMotion);
            menu.prepare();
            wait(100);
            compare(menu.renderedNotificationCount, 25);
            const preparedHeight = menu.stackHeight;
            verify(preparedHeight > 0);
            menu.menuActive = true;
            wait(100);
            compare(menu.stackHeight, preparedHeight);
            compare(menu.renderedNotificationCount, 25);
            menu.menuActive = false;
            wait(300);
            compare(menu.renderedNotificationCount, 25);
            const snapshot = menu.preparedEntries;
            for (let i = 0; i < 100; i++)
                menu.entries = menu.entries.slice();
            wait(100);
            compare(menu.preparedEntries, snapshot);
            compare(menu.stackHeight, preparedHeight);
            compare(menu.renderedNotificationCount, 25);
            report.setText("FAILURES 0\n");
            wait(100);
            finish.start();
        }
    }
}
