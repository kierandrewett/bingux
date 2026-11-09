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
    // Red only while something is being recorded. Saving a finished recording
    // is neutral, so it does not read as a second Stop button.
    activityColor: finalizing ? Theme.savingIndicator : isRecording ? Theme.recordingIndicator : Theme.privacyIndicator
    iconName: capture.state === "countdown" ? "media-record-symbolic" : ""
    interactive: !finalizing && (capture.busy || privacy.available)
    trailingIcon: finalizing ? "document-save-symbolic" : "media-playback-stop-symbolic"
    label: capture.recording ? capture.elapsedText + "  Stop" : finalizing ? "Saving…" : capture.state === "countdown" ? "Starting in " + capture.countdown : capture.state === "starting" ? "Starting…" : privacy.elapsedText
    tooltip: finalizing ? "Saving screen recording" : capture.busy && !capture.recording ? "Cancel capture" : hoveredAt > 0 ? "Stop at " + hoveredTime + " · Trim this hover and click" : "Stop screen recording"
    onClicked: {
        if (capture.busy)
            capture.stop(hoveredAt);
        else
            privacy.stopRecording();
    }
}
