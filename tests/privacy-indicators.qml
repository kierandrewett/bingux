import QtQuick
import QtQuick.Layouts
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: test
    property int phase: 0
    property int failures: 0
    property int stopped: 0
    function check(ok, message) { if (!ok) { failures++; console.error("FAIL: " + message); } }
    QtObject {
        id: fixtureCapture
        property string state: "idle"
        readonly property bool recording: state === "recording"
        readonly property bool busy: ["starting", "recording", "finalizing"].includes(state)
        property string elapsedText: "1:23"
        function stop() { test.stopped++; state = "finalizing"; }
    }
    QtObject {
        id: fixturePrivacy
        property bool available: true
        property bool recording: false
        property bool cameraInUse: false
        property bool screenSharing: false
        property string elapsedText: "2:45"
        function stopRecording() { test.stopped++; fixturePrivacy.recording = false; }
        function stopSharing() { test.stopped++; fixturePrivacy.screenSharing = false; }
    }
    QtObject {
        id: fixtureMetrics
        property bool screenSharing: false
        property bool microphoneInUse: false
        property bool locationInUse: false
    }
    PrivacyState { id: clockState }
    PanelWindow {
        id: panel
        anchors { top: true; left: true }
        implicitWidth: 640
        implicitHeight: 64
        color: Theme.barBackground
        RowLayout {
            anchors.centerIn: parent
            spacing: Theme.gap
            RecordingIndicator { id: recording; capture: fixtureCapture; privacy: fixturePrivacy; barWindow: panel }
            PrivacyIndicators { id: indicators; systemMetrics: fixtureMetrics; privacyState: fixturePrivacy; barWindow: panel }
        }
    }
    Timer {
        interval: 350
        running: true
        repeat: true
        onTriggered: {
            if (test.phase === 0) {
                test.check(!recording.visible && indicators.implicitWidth === 0, "Inactive indicators occupy no space");
                fixtureCapture.state = "recording";
                fixturePrivacy.cameraInUse = true;
                fixturePrivacy.screenSharing = true;
                fixtureMetrics.microphoneInUse = true;
                fixtureMetrics.locationInUse = true;
                clockState.apply({recording: true, recordingCount: 1, recordingElapsed: 65, cameraInUse: false, screenSharing: false});
            } else if (test.phase === 1) {
                test.check(recording.visible && recording.label === "1:23", "Own recording displays elapsed time");
                test.check(recording.filled && recording.activityColor.toString() === "#c01c28", "Recording uses GNOME red");
                test.check(recording.trailingIcon === "screencast-stop-symbolic" && recording.interactive, "Recording has an active stop control");
                test.check(recording.height === 32 && indicators.height === 32, "Full-height bar targets");
                test.check(indicators.visible && indicators.implicitWidth > 100 && indicators.sharingVisible, "Sharing, camera, microphone and location render together");
                recording.clicked();
                test.check(test.stopped === 1 && recording.label === "Saving…" && !recording.interactive, "Stop finalises once and disables repeated clicks");
            } else if (test.phase === 2) {
                fixtureCapture.state = "idle";
                fixturePrivacy.recording = true;
            } else if (test.phase === 3) {
                test.check(recording.label === "2:45" && recording.visible, "External recording uses compositor timer");
                recording.clicked();
                test.check(test.stopped === 2 && !fixturePrivacy.recording, "External recording stop reaches privacy service");
                test.check(clockState.elapsed >= 66 && clockState.elapsedText.startsWith("1:"), "Elapsed timer advances from compositor time");
                fixturePrivacy.screenSharing = false;
                test.check(indicators.sharingVisible, "Sharing retains GNOME minimum visible duration");
                fixtureMetrics.microphoneInUse = false;
                fixtureMetrics.locationInUse = false;
                fixturePrivacy.cameraInUse = false;
            } else if (test.phase === 4) {
                clockState.apply({recording: false, recordingCount: 0, recordingElapsed: 0, cameraInUse: false, screenSharing: false});
                test.check(!clockState.recording && clockState.elapsed === 0, "Stopping resets elapsed state");
                clockState.apply({recording: "bad"});
                test.check(!clockState.recording, "Malformed state does not activate indicators");
            } else if (test.phase === 17) {
                test.check(indicators.implicitWidth === 0 && !recording.visible, "Indicators disappear after activity and hold time end");
                console.info(test.failures ? "PRIVACY_UI_FAILED" : "PRIVACY_UI_PASSED");
                Qt.quit();
            }
            test.phase++;
        }
    }
}
