import QtQuick
import Quickshell
import Quickshell.Wayland

// Holds the most recent completed workspace change. Each change replaces the
// previous one and restarts the hold, so rapid switches never queue labels.
QtObject {
    id: root

    // Any object with a workspaceActivated(workspace, sequence) signal.
    property var source: null
    readonly property int holdDuration: 1200
    property var workspace: null
    property int sequence: -1
    property string outputName: ""
    property string lastOutputName: ""
    property bool shown: false

    function isWorkspace(value) {
        return !!value && typeof value === "object" && typeof value.id === "string" && value.id.length > 0 && Number.isSafeInteger(value.number) && value.number >= 1 && typeof value.name === "string";
    }

    // The focused window's output stands in for the active output. When no
    // window is focused, the last known output stays in use.
    function activeOutputName() {
        const toplevel = ToplevelManager.activeToplevel;
        const screen = toplevel && toplevel.screens && toplevel.screens.length > 0 ? toplevel.screens[0] : null;
        if (screen && screen.name)
            lastOutputName = screen.name;
        if (lastOutputName)
            return lastOutputName;
        return Quickshell.screens.length > 0 ? Quickshell.screens[0].name : "";
    }

    function accept(value, eventSequence) {
        if (!isWorkspace(value))
            return false;
        if (Number.isSafeInteger(eventSequence) && eventSequence >= 0 && eventSequence < sequence)
            return false;
        workspace = {
            id: value.id,
            number: value.number,
            name: value.name
        };
        if (Number.isSafeInteger(eventSequence) && eventSequence >= 0)
            sequence = eventSequence;
        outputName = activeOutputName();
        shown = true;
        hold.restart();
        return true;
    }

    property var sourceConnection: Connections {
        target: root.source
        ignoreUnknownSignals: true
        function onWorkspaceActivated(value, eventSequence) {
            root.accept(value, eventSequence);
        }
    }

    property var hold: Timer {
        interval: root.holdDuration
        repeat: false
        onTriggered: root.shown = false
    }
}
