import QtQuick

ActivityIndicator {
    id: root
    required property var capture
    required property var privacy
    readonly property bool active: capture.busy || privacy.recording
    readonly property bool isRecording: capture.recording || privacy.recording
    readonly property bool finalizing: capture.state === "finalizing"
    property double hoveredAt: 0
    property string hoveredTime: ""
    onHoveredChanged: {
        hoveredAt = hovered && capture.recording && capture.cleanStop ? Date.now() : 0;
        hoveredTime = hoveredAt > 0 ? capture.elapsedText : "";
    }
    onIsRecordingChanged: hoveredAt = 0
    visible: active
    filled: true
    activityColor: isRecording || finalizing ? Theme.recordingIndicator : Theme.privacyIndicator
    interactive: !finalizing && (capture.busy || privacy.available)
    trailingIcon: finalizing ? "document-save-symbolic" : "screencast-stop-symbolic"
    label: capture.recording ? capture.elapsedText + "  Stop" : finalizing ? "Saving…" : capture.state === "countdown" ? String(capture.countdown) : capture.state === "starting" ? "Starting…" : privacy.elapsedText
    tooltip: finalizing ? "Saving screen recording" : capture.busy && !capture.recording ? "Cancel capture" : hoveredAt > 0 ? "Stop at " + hoveredTime + " · Trim this hover and click" : "Stop screen recording"
    onClicked: {
        if (capture.busy)
            capture.stop(hoveredAt);
        else
            privacy.stopRecording();
    }
}
