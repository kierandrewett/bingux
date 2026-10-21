import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CAPTURE_TEST_RESULTS")
    }
    Timer {
        id: finish
        interval: 300
        onTriggered: Qt.quit()
    }
    Timer {
        interval: 15000
        running: true
        onTriggered: Qt.quit()
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
    }
    TestCase {
        name: "CaptureRequests"
        when: true
        function test_request_bookkeeping() {
            report.setText("FAIL: request bookkeeping did not finish\n");
            compare(capture.requestStatus("never-seen").state, "unknown");
            compare(capture.requestIdError("bad id").length > 0, true, "Spaces are rejected");
            compare(capture.requestIdError("").length > 0, true, "Empty IDs are rejected");
            compare(capture.requestIdError("a".repeat(65)).length > 0, true, "Long IDs are rejected");
            compare(capture.requestIdError("photo-1.png_x"), "", "Allowed characters pass");
            compare(capture.cancelRequest("photo-1").ok, false, "Cancel without a pending request fails");
            capture.rememberRequest("done-1", "saved", "/tmp/x.png", "");
            compare(capture.requestStatus("done-1").state, "saved");
            compare(capture.requestStatus("done-1").path, "/tmp/x.png");
            compare(capture.requestIdError("done-1"), "Request ID already used", "Finished IDs stay reserved");
            capture.rememberRequest("active-1", "pending");
            capture.activeRequest = "active-1";
            compare(capture.requestStatus("active-1").state, "pending");
            compare(capture.cancelRequest("other").ok, false, "Cancel must name the active request");
            compare(capture.cancelRequest("active-1").ok, true, "Cancel names the active request");
            capture.finishRequest("cancelled");
            compare(capture.requestStatus("active-1").state, "cancelled");
            compare(capture.activeRequest, "");
            report.setText("PASS: request ID validation, duplicate reservation, pending and cancelled states, unknown IDs\n");
            finish.start();
        }
    }
}
