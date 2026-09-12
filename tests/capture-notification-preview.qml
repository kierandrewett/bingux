import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    Timer {
        id: finish
        interval: 100
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
        function setPaused(notification, paused) {
        }
        function canActivate(entry) {
            return false;
        }
        property var allEntries: []
    }
    PanelWindow {
        implicitWidth: 420
        implicitHeight: 500
        color: "transparent"
        NotificationStack {
            id: stack
            anchors.fill: parent
            state: state
            animationsEnabled: false
            presentedEntries: [
                {
                    notification: {
                        id: 1
                    },
                    appName: "Capture",
                    desktopEntry: "",
                    appIcon: "screenshot-selection-symbolic",
                    summary: "Screenshot saved",
                    body: "Window screenshot.png",
                    image: Quickshell.env("CAPTURE_PREVIEW_IMAGE"),
                    actions: ["copy", "save", "discard"].map(id => ({
                                action: {
                                    identifier: id,
                                    invoke: () => {}
                                },
                                text: {
                                    copy: "Copy",
                                    save: "Save As",
                                    discard: "Discard"
                                }[id],
                                defaultAction: false
                            })),
                    receivedAt: Date.now(),
                    timeoutMs: 0
                }
            ]
        }
    }
    TestCase {
        when: true
        function cleanupTestCase() {
            finish.start();
        }
        function test_preview() {
            wait(600);
            const preview = findChild(stack, "notificationImagePreview");
            verify(preview !== null);
            tryCompare(preview, "status", Image.Ready, 4000);
            verify(preview.largePreview);
            verify(preview.width > 300);
            verify(preview.height <= 240);
            verify(Math.abs(preview.paintedWidth - preview.width) < 1 || Math.abs(preview.paintedHeight - preview.height) < 1);
            const card = findChild(stack, "notificationCard");
            let captured = false;
            card.grabToImage(image => {
                image.saveToFile("/tmp/capture-notification-card.png");
                captured = true;
            });
            tryVerify(() => captured, 2000);
            report.setText("FAILURES 0\nCapture image fills its preview without distortion\n");
        }
    }
}
