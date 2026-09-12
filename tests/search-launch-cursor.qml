import QtQuick
import Quickshell

ShellRoot {
    id: test
    property int step: 0
    property string token: ""
    property string timedOutRequest: ""
    property var launches: []
    property var sample: ({
            resultId: "test-app",
            title: "Test Application",
            kind: "application",
            providerId: "applications",
            desktopId: "test-app.desktop"
        })
    function check(condition, message) {
        if (!condition) {
            console.error("FAIL: " + message);
            Qt.exit(1);
        }
    }
    function start() {
        search.showSearch();
        search.awaitingResults = false;
        search.activateResult(sample);
        test.token = search.launchFeedbackToken;
        test.check(!!test.token && LaunchFeedback.active.length === 1, "search requests global cursor feedback");
        test.check(test.launches.length > 0 && test.launches[test.launches.length - 1].id === sample.desktopId, "search starts the matching dock loading pill");
    }
    SearchOverlay {
        id: search
        dockView: dockProxy
    }
    QtObject {
        id: dockProxy
        function beginExternalLaunch(id, name) {
            test.launches = test.launches.concat([
                {
                    id,
                    name
                }
            ]);
            return true;
        }
        function endExternalLaunch(id) {
            test.launches = test.launches.concat([
                {
                    id: "end:" + id
                }
            ]);
        }
    }
    Timer {
        id: steps
        interval: 200
        running: true
        repeat: true
        onTriggered: {
            switch (test.step++) {
            case 0:
                test.start();
                break;
            case 1:
                test.check(search.launchCursorActive, "cursor feedback remains pending");
                search.testSocket.requestFailed(search.activeActivationRequestId, "unknown-result");
                test.check(!search.activationPending && LaunchFeedback.active.length === 0, "failure ends global cursor feedback");
                test.start();
                break;
            case 2:
                search.testSocket.activationCompleted(search.activeActivationRequestId);
                test.check(!search.activationPending && search.launchFeedbackToken === "", "success releases local request state");
                test.check(LaunchFeedback.active.length === 1, "global cursor survives popup closing until app maps");
                LaunchFeedback.end(test.token);
                break;
            case 3:
                test.check(!search.visible, "search closes after successful dispatch");
                test.start();
                break;
            case 4:
                search.testSocket.connectionState = "unavailable";
                test.check(!search.activationPending && LaunchFeedback.active.length === 0, "disconnect restores global cursor");
                search.testSocket.connectionState = "ready";
                test.start();
                Qt.callLater(() => {
                    test.timedOutRequest = search.activeActivationRequestId;
                });
                steps.interval = Theme.launchTimeout + 150;
                break;
            case 5:
                test.check(!search.activationPending && LaunchFeedback.active.length === 0, "timeout restores global cursor");
                test.check(search.testSocket.cancelled.indexOf(test.timedOutRequest) >= 0, "timeout cancels the pending activation");
                search.testSocket.activationCompleted(test.timedOutRequest);
                test.check(search.visible && !search.closing, "late completion cannot close a retriable search");
                steps.interval = 200;
                break;
            case 6:
                test.start();
                search.closeSearch();
                test.check(LaunchFeedback.active.length === 0, "manual close cancels global launch feedback");
                console.info("SEARCH_CURSOR_PASSED");
                Qt.quit();
            }
        }
    }
}
