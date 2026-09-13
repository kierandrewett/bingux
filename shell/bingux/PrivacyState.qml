import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    property bool available: false
    property bool screenSharing: false
    property bool cameraInUse: false
    property var cameraCaptures: []
    property bool microphoneAvailable: false
    property var microphoneCaptures: []
    property var locationCaptures: []
    readonly property bool microphoneInUse: microphoneCaptures.length > 0
    function desktopEntryForCapture(capture) {
        const id = String(capture?.appId || "").trim();
        if (!id)
            return null;
        const withoutSuffix = id.replace(/\.desktop$/i, "");
        const direct = DesktopEntries.byId(withoutSuffix) || DesktopEntries.byId(id) || DesktopEntries.heuristicLookup(withoutSuffix);
        if (direct)
            return direct;
        const compact = value => String(value || "").toLowerCase().replace(/[^a-z0-9]/g, "");
        const compactId = compact(withoutSuffix);
        return Array.from(DesktopEntries.applications.values).find(entry => [entry.id, entry.startupClass, entry.name].some(value => compact(value) === compactId)) || null;
    }
    function detailForCapture(capture) {
        const entry = desktopEntryForCapture(capture);
        return Object.assign({}, capture, {
            app: entry?.name || capture.app || "Unknown application",
            appIcon: entry?.icon || "application-x-executable"
        });
    }
    readonly property var microphoneDetails: microphoneCaptures.map(detailForCapture)
    readonly property string microphoneTooltip: "Microphone in use"
    readonly property var cameraDetails: cameraCaptures.map(detailForCapture)
    readonly property string cameraTooltip: "Camera in use"
    readonly property var locationDetails: locationCaptures.map(detailForCapture)
    readonly property string locationTooltip: "Location in use"
    Process {
        command: ["python3", Qt.resolvedUrl("microphone-status.py").toString().replace("file://", "")]
        running: true
        stdout: SplitParser {
            onRead: data => {
                try {
                    const state = JSON.parse(data);
                    root.microphoneAvailable = state.available === true;
                    root.microphoneCaptures = state.captures || [];
                } catch (error) {
                    console.warn("Microphone state:", error);
                }
            }
        }
    }
    Process {
        command: ["python3", Qt.resolvedUrl("camera-status.py").toString().replace("file://", "")]
        running: true
        stdout: SplitParser {
            onRead: data => {
                try {
                    const state = JSON.parse(data);
                    root.cameraCaptures = state.captures || [];
                } catch (error) {
                    console.warn("Camera state:", error);
                }
            }
        }
    }
    property bool recording: false
    property int recordingCount: 0
    property int elapsed: 0
    property double recordingOrigin: 0
    readonly property string elapsedText: Math.floor(elapsed / 60) + ":" + String(elapsed % 60).padStart(2, "0")
    function stopSharing() {
        connection.send({
            op: "stop-sharing"
        });
    }
    function stopRecording() {
        connection.send({
            op: "stop-recording"
        });
    }
    function apply(state) {
        if (!state || [state.screenSharing, state.cameraInUse, state.recording].some(value => typeof value !== "boolean") || !Number.isInteger(state.recordingElapsed) || state.recordingElapsed < 0 || !Number.isInteger(state.recordingCount) || state.recordingCount < 0)
            return;
        available = true;
        screenSharing = state.screenSharing;
        cameraInUse = state.cameraInUse;
        locationCaptures = Array.isArray(state.locationCaptures) ? state.locationCaptures : [];
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
        function status(): string {
            return JSON.stringify({
                available: root.available,
                screenSharing: root.screenSharing,
                microphoneAvailable: root.microphoneAvailable,
                microphoneCaptures: root.microphoneCaptures,
                cameraInUse: root.cameraInUse,
                locationCaptures: root.locationCaptures,
                recording: root.recording,
                elapsed: root.elapsed
            });
        }
    }
}
