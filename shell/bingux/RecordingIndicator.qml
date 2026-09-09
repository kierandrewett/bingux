import QtQuick

ActivityIndicator {
    id: root
    required property var capture
    required property var privacy
    readonly property bool active: capture.busy || privacy.recording
    readonly property bool isRecording: capture.recording || privacy.recording
    readonly property bool finalizing: capture.state === "finalizing"
    visible: active
    filled: true
    activityColor: isRecording || finalizing ? Theme.recordingIndicator : Theme.privacyIndicator
    interactive: !finalizing && (capture.busy || privacy.available)
    trailingIcon: finalizing ? "document-save-symbolic" : "screencast-stop-symbolic"
    label: capture.recording ? capture.elapsedText : finalizing ? "Saving…"
        : capture.state === "countdown" ? String(capture.countdown)
        : capture.state === "starting" ? "Starting…" : privacy.elapsedText
    tooltip: finalizing ? "Saving screen recording"
        : capture.busy && !capture.recording ? "Cancel capture" : "Stop screen recording"
    onClicked: {
        if (capture.busy) capture.stop();
        else privacy.stopRecording();
    }
}
