import QtQuick
import Quickshell.Services.Pipewire

// Volume feedback for the lock screen. Gnoblin's session lock isolates
// shortcut paths, so its key bindings do not run while locked. The volume keys
// reach this client as ordinary key presses, and the desktop shell's OSD is
// hidden behind the lock surface. This
// item changes the default output or input itself, as the session bar slider
// does, and shows an OsdCard for the records that bingux-statusd publishes.
//
// The key press only changes the audio node. The card comes from the same
// statusd record the desktop shows, so the lock screen has one OSD path and
// also reports changes made with the session bar slider. A key that changes
// nothing, for example volume up at the maximum, shows no card.
//
// The host forwards key events with `Keys.forwardTo: [osd]`.
Item {
    id: root
    objectName: "sessionOsd"

    // An OsdState connected to bingux-statusd. Null turns the item off.
    property var osdState: null
    // The output this surface covers, as the compositor names it.
    property string outputName: ""
    property var sink: Pipewire.defaultAudioSink
    property var source: Pipewire.defaultAudioSource
    // The same step as the Gnoblin key binding: 5 percent.
    readonly property real volumeStep: 0.05
    readonly property alias card: card

    function usable(node) {
        return root.osdState !== null && node !== null && node.ready && node.audio !== null && SessionServices.allows("volume");
    }

    function stepVolume(delta) {
        if (!usable(sink))
            return;
        const audio = sink.audio;
        audio.muted = false;
        audio.volume = Math.max(0, Math.min(1, Math.round((audio.volume + delta) * 100) / 100));
    }

    function toggleMute(node) {
        if (usable(node))
            node.audio.muted = !node.audio.muted;
    }

    // True when the key belongs to this item, whether or not it changed audio.
    function handleKey(key) {
        switch (key) {
        case Qt.Key_VolumeUp:
            stepVolume(volumeStep);
            return true;
        case Qt.Key_VolumeDown:
            stepVolume(-volumeStep);
            return true;
        case Qt.Key_VolumeMute:
            toggleMute(sink);
            return true;
        case Qt.Key_MicMute:
            toggleMute(source);
            return true;
        }
        return false;
    }

    Keys.onPressed: event => {
        if (root.osdState !== null)
            event.accepted = root.handleKey(event.key);
    }

    PwObjectTracker {
        objects: [Pipewire.defaultAudioSink, Pipewire.defaultAudioSource].filter(node => node !== null)
    }

    // Level with the middle of the screen: the top holds the bar and clock,
    // the bottom holds the prompt, and the middle is free on every output.
    OsdCard {
        id: card
        z: 30
        request: root.osdState !== null ? root.osdState.requestForOutputName(root.outputName) : null
        availableWidth: root.width
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        color: Qt.rgba(0, 0, 0, 0.42)
        border.color: Qt.rgba(1, 1, 1, 0.16)
        ink: "#ffffff"
        inkMuted: Qt.rgba(1, 1, 1, 0.72)
        accent: "#ffffff"
        tileColor: Qt.rgba(1, 1, 1, 0.12)
        trackColor: Qt.rgba(1, 1, 1, 0.22)
        panelOutline: false
        visible: opacity > 0
    }
}
