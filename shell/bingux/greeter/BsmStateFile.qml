import QtQuick
import Quickshell
import Quickshell.Io

// Writes the greeter's non-secret state to $XDG_RUNTIME_DIR/bsm-greeter-state.json
// whenever it changes, so a test harness (or an admin with root) can see why a
// control is hidden or a status shows. bsm sends the greeter's stderr to
// /dev/null, so a file is the only channel. It holds booleans, ids and phases;
// never a prompt text, a typed answer, an account name or a path.
Item {
    id: root

    required property var model
    readonly property string runtime: {
        const dir = Quickshell.env("XDG_RUNTIME_DIR");
        return typeof dir === "string" ? dir : "";
    }
    readonly property string snapshot: JSON.stringify({
        connected: model.connected,
        synced: model.synced,
        revision: model.revision,
        mode: model.modeKind,
        users: model.users.length,
        availability: model.users.map(u => u.availability),
        selectedUid: model.selectedUid,
        phase: model.phase,
        canSwitchUser: model.canSwitchUser,
        canAuthenticate: model.canAuthenticate,
        showProgress: model.showProgress,
        signingIn: model.signingIn,
        quiet: model.quiet,
        operation: {
            id: model.operationId,
            kind: model.operationKind,
            phase: model.operationPhase,
            transitionRunning: model.transitionRunning
        },
        busy: model.busy,
        gesture: model.gesture
    })

    FileView {
        id: file
        path: root.runtime !== "" ? root.runtime + "/bsm-greeter-state.json" : ""
        atomicWrites: true
        printErrors: false
    }
    onSnapshotChanged: if (root.runtime !== "")
        file.setText(root.snapshot + "\n")
    Component.onCompleted: if (root.runtime !== "")
        file.setText(root.snapshot + "\n")
}
