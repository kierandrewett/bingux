import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    NotificationState { id: notifications }
    NotificationSurface { id: notificationSurface; state: notifications }
    CaptureTool { id: capture; screen: Quickshell.screens[0] }
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
        id: lateWindow
        property string title: "Late window"
        property string appId: "bingux-missing-launch-test"
        property bool activated: false
        property bool minimized: false
    }
    Dock {
        id: dock
        visible: false
        settings: ({
                pinnedApps: []
            })
    }
    TestCase {
        when: true
        function cleanupTestCase() {
            finish.start();
        }
        function test_failures() {
            report.setText("FAIL: initial launch\n");
            wait(300);
            const id = "bingux-missing-launch-test";
            const group = {
                id: id,
                desktopEntry: {
                    id: id,
                    name: "Launch test"
                },
                windows: []
            };
            dock.appGroups = [group];
            dock.visible = true;
            dock.launch(group, false);
            tryVerify(() => !!dock.launchFailures[id], 10000);
            const dialog = findChild(dock, "launchErrorDialog");
            report.setText("FAIL: notification delivery " + JSON.stringify(notifications.allEntries.map(entry => ({summary:entry.summary, body:entry.body}))) + "\n");
            verify(dialog === null, "Launch errors must not create a modal popup");
            tryVerify(() => notifications.allEntries.some(entry => entry.summary === "Could not open Launch test"), 5000);
            const notice = notifications.allEntries.find(entry => entry.summary === "Could not open Launch test");
            verify(notice.body.includes("could not be found"));
            compare(notice.actions[0].action.identifier, "retry");
            report.setText("FAIL: failed icon appearance\n");
            verify(!dock.launchAttempts[id]);
            const icon = findChild(dock.contentItem, "dockApplicationIcon");
            verify(icon !== null);
            compare(icon.opacity, 0.5);
            verify(icon.layer.enabled);
            report.setText("FAIL: notification screenshot\n");
            let captured = false;
            notificationSurface.viewport.grabToImage(result => {
                result.saveToFile("/tmp/bingux-launch-error.png");
                captured = true;
            });
            tryVerify(() => captured, 2000);
            verify(!!dock.launchFailures[id]);
            report.setText("FAIL: concurrent launches\n");
            dock.launchAttempts = {
                first: {
                    serial: 100,
                    windows: [],
                    deadline: Date.now() + 5000
                },
                second: {
                    serial: 101,
                    windows: [],
                    deadline: Date.now() + 5000
                }
            };
            dock.failLaunch("first", 99, "Stale error");
            verify(!dock.launchFailures.first);
            dock.failLaunch("first", 100, "First failure");
            verify(!!dock.launchAttempts.second);
            dock.failLaunch("second", 101, "Second failure");
            tryVerify(() => notifications.allEntries.some(entry => entry.body === "First failure"), 5000);
            tryVerify(() => notifications.allEntries.some(entry => entry.body === "Second failure"), 5000);
            dock.appGroups = [
                {
                    id: id,
                    desktopEntry: group.desktopEntry,
                    windows: [lateWindow]
                }
            ];
            report.setText("FAIL: late recovery\n");
            tryVerify(() => !dock.launchFailures[id], 1000);
            compare(icon.opacity, 1);
            dock.launchAttempts = {
                timeout: {
                    serial: 200,
                    windows: [],
                    deadline: Date.now() - 1
                }
            };
            report.setText("FAIL: timeout notification\n");
            tryVerify(() => !!dock.launchFailures.timeout, 1000);
            tryVerify(() => notifications.allEntries.some(entry => entry.body.includes("may still be starting")), 5000);
            dock.launchFailures = Object.assign({}, dock.launchFailures, {
                timeout: Object.assign({}, dock.launchFailures.timeout, {
                    expiresAt: Date.now() - 1
                })
            });
            tryVerify(() => !dock.launchFailures.timeout, 1000);
            report.setText("FAIL: capture error notification\n");
            capture.handle({event: "error", message: "The recording worker stopped unexpectedly.", partial: "/tmp/preserved-recording.mp4"});
            capture.handle({event: "error", message: "The recording worker stopped unexpectedly.", partial: "/tmp/preserved-recording.mp4"});
            tryVerify(() => notifications.allEntries.some(entry => entry.summary === "Capture failed"), 5000);
            const captureErrors = notifications.allEntries.filter(entry => entry.summary === "Capture failed");
            compare(captureErrors.length, 1, "Replayed error is not notified twice");
            verify(captureErrors[0].body.includes("/tmp/preserved-recording.mp4"));
            report.setText("FAIL: retry action\n");
            // Retry dispatch itself is unit-tested without launching into the
            // real host user manager from this isolated notification session.
            report.setText("FAILURES 0\nNormal notifications: missing entry, Retry button, capture error and partial path, deduplication, grey icon, late recovery, timeout and concurrent launches verified\n");
            finish.start();
        }
    }
}
