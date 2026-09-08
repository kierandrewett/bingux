import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    property var activeStreams: []
    Process {
        id: meter
        command: [Quickshell.env("BINGUX_AUDIO_METER") || "bingux-audio-meter"]
        running: true
        stdout: SplitParser {
            onRead: data => {
                try {
                    const streams = JSON.parse(data);
                    if (Array.isArray(streams)) root.activeStreams = streams;
                } catch (_) {}
            }
        }
        onExited: { root.activeStreams = []; reconnect.restart(); }
    }
    Timer { id: reconnect; interval: 3000; onTriggered: meter.running = true }
}
