import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    QtObject {
        id: source
        property var allEntries: Array.from({length: 100}, (_, i) => ({notification: {id: i + 1}, appName: "Downloads", desktopEntry: "", appIcon: "", summary: (i % 3 === 0 ? "Screenshot " : "Download ") + (i + 1), body: i % 2 ? "Short message" : "A completed download with enough text to exercise the card layout.", actions: [], receivedAt: Date.now(), timeoutMs: 0, image: i % 3 === 0 ? "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='300' height='150'%3E%3Crect width='300' height='150' fill='%233a658b'/%3E%3C/svg%3E" : ""}))
        function setPaused(notification, paused) {}
    }
    QtObject {
        id: briefSource
        property var allEntries: source.allEntries.slice(0, 3)
        property var visibleEntries: allEntries
        function setPaused(notification, paused) {}
        function archiveToasts() {}
    }
    QtObject {
        id: centre
        property bool retained: false
        property bool visible: false
        property int listHeight: 500
        property int listX: 0
        property int listY: 0
        property var body: historyBody
    }
    Item { id: historyBody; parent: window.contentItem }
    NotificationSurface { id: surface; state: briefSource; notificationCentre: centre; inputSuspended: true }
    Window {
        id: window
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        width: 420
        height: 640
        color: Theme.barBackground
        NotificationStack { id: stack; anchors.fill: parent; anchors.margins: 16; state: source; presentedEntries: source.allEntries; historyMode: true }
        TestCase {
            when: window.visible
            property string checks: ""
            property int failures: 0
            function check(value, message) { checks += "CHECK " + value + " " + message + "\n"; if (!value) failures++; report.setText(checks + "FAILURES " + failures + "\n"); }
            function cards(item) {
                let result = [];
                for (const child of item.children || []) {
                    if (child.objectName === "notificationCard") result.push(child);
                    else result = result.concat(cards(child));
                }
                return result;
            }
            function test_large_group() {
                wait(400);
                const rows = cards(stack.contentItem);
                check(rows.length === 100, "All 100 notifications remain available");
                let toasts = cards(surface.viewport.contentItem);
                check(toasts.length === 3 && toasts.every(row => row.groupCount === 1 && !row.collapsedStack), "Incoming notifications remain separate cards");
                centre.retained = true;
                wait(80);
                let history = cards(surface.viewport.contentItem);
                check(history.length === 3 && history.every(row => row.groupCount === 3), "Opening the centre groups notifications by app");
                centre.retained = false;
                wait(80);
                check(cards(surface.viewport.contentItem).every(row => row.groupCount === 1), "Returning to toasts restores separate cards");
                const preview = findChild(rows[0], "notificationImagePreview");
                tryCompare(preview, "status", Image.Ready);
                const decodeWidth = preview.sourceSize.width;
                const start = Date.now();
                stack.toggleGroup("Downloads");
                const toggleTime = Date.now() - start;
                wait(40);
                const rendered = rows.filter(row => row.visible).length;
                check(rendered <= 14, "Expansion renders only nearby cards: " + rendered + "; toggle cost " + toggleTime + "ms");
                let decodeChanged = preview.sourceSize.width !== decodeWidth;
                let loadingFrames = 0;
                for (let frame = 0; frame < 20; frame++) {
                    wait(16);
                    decodeChanged = decodeChanged || preview.sourceSize.width !== decodeWidth;
                    if (preview.status !== Image.Ready) loadingFrames++;
                }
                check(!decodeChanged, "Expansion keeps the decoded image size stable");
                check(loadingFrames === 0, "Preview stays ready throughout expansion: " + loadingFrames + " loading frames");
                const offset = stack.contentY;
                mouseWheel(stack, 120, 180, 0, -120);
                wait(50);
                const distance = stack.contentY - offset;
                wait(160);
                check(Math.abs(stack.contentY - offset - 160) < 1, "Wheel notch settles at 160px");
                const points = [Qt.point(4, 180), Qt.point(120, 40), Qt.point(120, 180), Qt.point(120, 350), Qt.point(stack.width - 2, 180)];
                for (let i = 0; i < points.length; i++) {
                    stack.cancelFlick();
                    stack.contentY = 500;
                    mouseWheel(stack, points[i].x, points[i].y, 0, -120);
                    const immediate = stack.contentY;
                    wait(180);
                    check((Theme.reducedMotion || immediate < 660) && Math.abs(stack.contentY - 660) < 1,
                        "Consistent eased wheel step at " + points[i] + ": " + immediate + " then " + stack.contentY);
                }
                stack.historyMode = false;
                stack.contentY = 500;
                mouseWheel(stack, 120, 180, 0, -120);
                const toastImmediate = stack.contentY;
                wait(180);
                check((Theme.reducedMotion || toastImmediate < 660) && Math.abs(stack.contentY - 660) < 1,
                    "Toast and history views use the same scroll motion: " + toastImmediate + " then " + stack.contentY);
                stack.historyMode = true;
                wait(250);
                stack.contentY = 500;
                for (let notch = 0; notch < 3; notch++) mouseWheel(stack, 120, 180, 0, -120);
                wait(180);
                check(Math.abs(stack.contentY - 980) < 1, "Rapid notches retain their full scroll distance");
                mouseWheel(stack, 120, 180, 0, -120);
                wait(30);
                mouseWheel(stack, 120, 180, 0, 120);
                wait(180);
                check(Math.abs(stack.contentY - 980) < 1, "Wheel direction can reverse during easing");
                const expandedHeight = stack.contentHeight;
                stack.contentY = Math.max(0, stack.contentHeight - stack.height);
                wait(50);
                check(rows[rows.length - 1].visible, "Last card is rendered when scrolled into view");
                check(Math.abs(stack.contentHeight - expandedHeight) < 1, "Scrolling mixed image and text cards preserves layout height");
                stack.contentY = 0;
                stack.toggleGroup("Downloads");
                wait(80);
                stack.toggleGroup("Downloads");
                wait(320);
                check(rows.filter(row => row.visible).length <= 14, "Interrupted expansion keeps rendering bounded");
                for (let i = 1; i < rows.length; i++) {
                    if (rows[i].y + 1 < rows[i-1].y + rows[i-1].height) { check(false, "Expanded cards overlap at " + i); break; }
                }
                check(stack.expandedApps.Downloads, "Rapid toggles settle expanded");
                grabImage(stack).save("/tmp/bingux-notification-performance.png");
                report.setText(checks + "FAILURES " + failures + "\n");
            }
            function cleanupTestCase() { window.visible = false; finish.start(); }
        }
    }
}
