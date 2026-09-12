import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    PanelWindow {
        id: host
        implicitWidth: 100
        implicitHeight: 100
    }
    QtObject {
        id: state
        property var allEntries: Array.from({
            length: 200
        }, (_, i) => ({
                    notification: {
                        id: i + 1
                    },
                    appName: "Downloads",
                    desktopEntry: "",
                    appIcon: "",
                    summary: "Notification " + i,
                    body: "Saved history",
                    actions: [],
                    receivedAt: Date.now(),
                    timeoutMs: 0,
                    image: "",
                    toastVisible: false
                }))
        readonly property var visibleEntries: allEntries.filter(entry => entry.toastVisible)
        function setPaused(notification, paused) {
        }
        function archiveToasts() {
        }
        function expiryProgress(entry) {
            return 0;
        }
        function canActivate(entry) {
            return false;
        }
    }
    QtObject {
        id: centre
        property bool retained: false
        property bool visible: false
        property int listHeight: 500
        property int listX: 0
        property int listY: 0
        property int panelX: 0
        property int panelY: 0
        property int popupWidth: 400
        property int popupHeight: 500
        property var body: historyBody
    }
    Item {
        id: historyBody
        parent: host.contentItem
    }
    NotificationSurface {
        id: surface
        state: state
        notificationCentre: centre
        inputSuspended: true
    }
    TestCase {
        parent: host.contentItem
        when: host.visible
        function initTestCase() {
            report.setText("RUNNING");
        }
        function cleanupTestCase() {
            report.setText("FAILURES " + qtest_results.failCount);
        }
        function test_history_releases_cards_after_close() {
            tryCompare(surface.viewport, "instantiatedCardCount", 0);
            surface.prepareForHistory();
            tryCompare(surface.viewport, "instantiatedCardCount", 200);
            centre.retained = true;
            centre.visible = true;
            compare(surface.viewport.instantiatedCardCount, 200);
            centre.visible = false;
            compare(surface.viewport.instantiatedCardCount, 200, "Keep cards during the closing slide");
            centre.retained = false;
            tryCompare(surface.viewport, "instantiatedCardCount", 0, 3000);
            compare(state.allEntries.length, 200, "History data survives card disposal");
            surface.prepareForHistory();
            centre.retained = true;
            tryCompare(surface.viewport, "instantiatedCardCount", 200);
            centre.retained = false;
        }
    }
}
