import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    property bool available: false
    property bool screenSharing: false
    property bool cameraInUse: false
    property bool recording: false
    property int recordingCount: 0
    property int elapsed: 0
    property double recordingOrigin: 0
    readonly property string elapsedText: Math.floor(elapsed / 60) + ":" + String(elapsed % 60).padStart(2, "0")
    function stopSharing() { connection.send({op: "stop-sharing"}); }
    function stopRecording() { connection.send({op: "stop-recording"}); }
    function apply(state) {
        if (!state || [state.screenSharing, state.cameraInUse, state.recording].some(value => typeof value !== "boolean")
            || !Number.isInteger(state.recordingElapsed) || state.recordingElapsed < 0
            || !Number.isInteger(state.recordingCount) || state.recordingCount < 0) return;
        available = true;
        screenSharing = state.screenSharing;
        cameraInUse = state.cameraInUse;
        recording = state.recording;
        recordingCount = state.recordingCount;
        elapsed = state.recordingElapsed;
        recordingOrigin = Date.now() - elapsed * 1000;
    }
    ShortcutSession {
        id: connection
        enabled: false
        trackPrivacy: true
        onPrivacySnapshot: state => root.apply(state)
        onCancelled: root.available = false
        onFailed: message => console.warn("Privacy state:", message)
    }
    Timer {
        interval: 1000
        running: root.recording
        repeat: true
        onTriggered: root.elapsed = Math.max(0, Math.floor((Date.now() - root.recordingOrigin) / 1000))
    }
    IpcHandler {
        target: "privacy"
        function status(): string { return JSON.stringify({available: root.available, screenSharing: root.screenSharing,
            cameraInUse: root.cameraInUse, recording: root.recording, elapsed: root.elapsed}); }
    }
}
