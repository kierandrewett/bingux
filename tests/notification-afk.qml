import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    id: root

    FileView {
        id: results
        path: Quickshell.env("BINGUX_NOTIFICATION_TEST_RESULTS")
    }
    property string checks: ""

    QtObject {
        id: notification
        property int id: 501
        property real expireTimeout: 0.8
        property string appName: "Sender"
        property string desktopEntry: ""
        property string appIcon: ""
        property string summary: "Arrived while away"
        property string body: "This notification must remain visible after activity resumes."
        property var actions: []
        property bool tracked: false
        signal closed(int reason)
        function dismiss() {}
        function expire() {}
    }

    NotificationState {
        id: state
    }

    TestCase {
        name: "NotificationAfk"
        when: true

        function check(value, message) {
            root.checks += "CHECK " + value + " " + message + "\n";
            results.setText(root.checks);
            verify(value, message);
        }

        function test_afk_toast_stays_until_dismissed() {
            state.isAfk = true;
            state.accept(notification);
            compare(state.visibleEntries.length, 1);
            compare(state.visibleEntries[0].timeoutMs, 0, "AFK notifications are untimed");
            compare(state.visibleEntries[0].deadline, 0, "AFK notifications have no expiry deadline");

            state.isAfk = false;
            notification.summary = "Updated after returning";
            state.resetExpiry(notification);
            wait(4500);
            check(state.visibleEntries.some(entry => entry.notification === notification), "the toast persists after returning and after a notification update");
            compare(state.visibleEntries[0].timeoutMs, 0, "updates do not restore an expiry timer");

            state.dismiss(notification);
            compare(state.visibleEntries.length, 0, "manual dismissal still removes the persistent toast");
            console.info("NOTIFICATION_AFK_PASSED");
        }

        function cleanupTestCase() {
            results.setText(root.checks + "FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }
    }
}
