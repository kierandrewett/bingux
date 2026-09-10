//@ pragma UseQApplication
import QtQuick
import Quickshell

ShellRoot {
    Connections { target: Quickshell; function onReloadCompleted() { Quickshell.inhibitReloadPopup(); } }
    UiSession {
        id: session
        sessionName: "emoji"
        bindings: [{id: "emoji", accelerator: Quickshell.env("BINGUX_EMOJI_SHORTCUT") || "<Super>period", hold: 67108864, modal: false}]
        onActivated: (id, first) => { if (first) { if (!picker.visible && !picker.locating) { picker.activationCount++; picker.open(); }; } }
        state: ({visible: picker.visible})
        onCommandReceived: command => {
            if (command.action === "open") picker.open();
            else if (command.action === "close") picker.close();
        }
    }
    EmojiPicker {
        id: picker
        screen: Quickshell.screens[0]
        // Quickshell keys state by the MD5 of the entry point path. Keep the
        // original desktop location so splitting the UI preserves preferences.
        stateDirectory: Quickshell.statePath("../" + Qt.md5(Quickshell.shellPath("shell.qml")) + "/emoji")
        onOpening: {
            session.command("search", {action: "close"});
            session.command("switcher", {action: "close"});
            session.command("capture", {action: "close"});
        }
    }
}
