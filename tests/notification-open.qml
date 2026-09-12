import QtQuick
import QtTest
import Quickshell

ShellRoot {
    QtObject {
        id: source
        property var allEntries: Array.from({
            length: 100
        }, (_, i) => ({
                    notification: {
                        id: i + 1
                    },
                    appName: "App " + (i % 10),
                    desktopEntry: "",
                    appIcon: "",
                    summary: "Notification " + i,
                    body: "Retained notification history",
                    actions: [],
                    receivedAt: Date.now(),
                    timeoutMs: 0,
                    image: "",
                    toastVisible: false
                }))
        property var visibleEntries: []
        function setPaused(notification, paused) {
        }
        function archiveToasts() {
        }
        function canActivate(entry) {
            return false;
        }
        function expiryProgress(notification) {
            return 1;
        }
    }
    NotificationSurface {
        id: surface
        state: source
        notificationCentre: history
    }
    NotificationHistoryPopup {
        id: history
        notificationSurface: surface
    }
    TestCase {
        name: "NotificationOpen"
        when: true
        function cards(item) {
            let result = [];
            for (const child of item.children || []) {
                if (child.objectName === "notificationCard")
                    result.push(child);
                else
                    result = result.concat(cards(child));
            }
            return result;
        }
        function check(value, message) {
            if (!value)
                console.error("FAIL: " + message);
            verify(value, message);
        }
        function test_reopen() {
            wait(300);
            const initial = cards(surface.viewport.contentItem);
            check(initial.length === 100, "History cards are prepared before opening");
            check(!surface.visible, "Archived history does not map the desktop surface");
            for (let opening = 0; opening < 3; ++opening) {
                const start = Date.now();
                history.visible = true;
                console.warn("OPEN_MS " + (Date.now() - start));
                const atOpen = cards(surface.viewport.contentItem);
                check(initial.every(card => atOpen.includes(card)), "Opening preserves every delegate");
                let animatedFrames = 0;
                for (let frame = 0; frame < 20; ++frame) {
                    wait(16);
                    if (history.slideOffset > 0)
                        animatedFrames++;
                }
                check(animatedFrames >= 3, "Opening renders intermediate slide frames");
                history.visible = false;
                tryCompare(history, "retained", false, 1000);
                const afterClose = cards(surface.viewport.contentItem);
                check(initial.every(card => afterClose.includes(card)), "Closing retains every delegate");
                check(!surface.visible, "Closing unmaps the desktop surface");
            }
            const toast = Object.assign({}, source.allEntries[0], {
                notification: {
                    id: 1001
                },
                toastVisible: true
            });
            source.allEntries = source.allEntries.concat([toast]);
            tryCompare(surface, "renderedNotificationCount", 1);
            wait(400);
            source.allEntries = source.allEntries.filter(entry => entry !== toast);
            check(surface.visible, "A retiring toast remains mapped for its exit");
            tryCompare(surface, "renderedNotificationCount", 0, 1000);
            check(!surface.visible, "The final toast unmaps after its exit");
            console.warn("PASS: retained history opens with intermediate animation frames");
        }
    }
}
