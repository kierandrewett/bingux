import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    UiSession {
        sessionName: "capture-recording-test"
        state: ({visible: capture.opened, surface: "bingux-capture", companions: ["bingux-capture-controls"], companionsAbove: true})
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
    }
    // Known backdrop proves exact scrim opacity and a clear recording region.
    PanelWindow {
        anchors { top: true; left: true }
        implicitWidth: 1000
        implicitHeight: 700
        color: "#808080"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-capture"
        mask: Region {}
    }
    QtObject {
        id: control
        readonly property bool busy: capture.busy
        readonly property bool recording: capture.recording
        readonly property bool cleanStop: !!capture.capabilities.cleanStop
        readonly property string state: capture.state
        readonly property string elapsedText: capture.elapsedText
        readonly property int countdown: capture.countdown
        function stop(hoveredAt) { capture.stop(hoveredAt); }
    }
    QtObject {
        id: privacy
        property bool recording: false
        property bool available: false
        property string elapsedText: "0:00"
    }
    PanelWindow {
        id: bar
        anchors { top: true; left: true }
        implicitWidth: 360
        implicitHeight: 40
        color: Theme.surface
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-capture-controls"
        RecordingIndicator {
            id: indicator
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            capture: control
            privacy: privacy
            barWindow: bar
        }
    }
    FileView { id: report; path: Quickshell.env("BINGUX_CAPTURE_TEST_RESULTS") }
    Process {
        id: screenshot
        command: ["grim", Quickshell.env("BINGUX_CAPTURE_TEST_GUIDE")]
    }
    Timer { interval: 20000; running: true; onTriggered: Qt.quit() }
    TestCase {
        name: "RecordingControls"
        parent: bar.contentItem
        when: capture.ready && bar.width > 0
        function test_recording() {
            report.setText("FAIL: recording controls\n");
            verify(capture.configureOptions(JSON.stringify({
                kind: "recording", target: "region", audio: "none", delay: 0,
                region: {x: 200, y: 200, width: 640, height: 360},
                directory: Quickshell.env("BINGUX_CAPTURE_TEST_DIRECTORY")
            })).ok);
            capture.open();
            tryCompare(capture, "opened", true, 4000);
            capture.take();
            tryCompare(capture, "recording", true, 5000);
            compare(capture.recordingTarget, "region");
            compare(capture.recordingRegion.width, 640);
            verify(control.cleanStop);
            wait(1500);
            screenshot.running = true;
            tryCompare(screenshot, "running", false, 3000);
            mouseMove(indicator, indicator.width / 2, indicator.height / 2);
            tryVerify(() => indicator.hoveredAt > 0, 1000);
            mouseMove(bar.contentItem, 340, 20);
            tryCompare(indicator, "hoveredAt", 0, 1000);
            wait(300);
            mouseMove(indicator, indicator.width / 2, indicator.height / 2);
            tryVerify(() => indicator.hoveredAt > 0, 1000);
            const marked = indicator.hoveredAt;
            const expected = (marked - capture.startedAt) / 1000;
            wait(1200);
            mouseClick(indicator, indicator.width / 2, indicator.height / 2);
            tryCompare(capture, "state", "saved", 6000);
            report.setText(JSON.stringify({path: capture.savedPath, expected, result: "PASS"}));
        }
    }
}
