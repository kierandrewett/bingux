//@ pragma UseQApplication
import QtQuick
import Quickshell

ShellRoot {
    Connections {
        target: Quickshell
        function onReloadCompleted() {
            Quickshell.inhibitReloadPopup();
        }
    }
    UiSession {
        id: session
        sessionName: "emoji"
        // Super+Period is registered by Gnoblin's native command-shortcut
        // integration. This client remains the IPC target for that command.
        bindings: []
        state: ({
                visible: picker.visible
            })
        onCommandReceived: command => {
            if (command.action === "open")
                picker.open();
            else if (command.action === "close")
                picker.close();
        }
    }
    EmojiPicker {
        id: picker
        screen: Quickshell.screens[0]
        // Quickshell keys state by the MD5 of the entry point path. Keep the
        // original desktop location so splitting the UI preserves preferences.
        stateDirectory: Quickshell.statePath("../" + Qt.md5(Quickshell.shellPath("shell.qml")) + "/emoji")
        onOpening: {
            session.command("search", {
                action: "close"
            });
            session.command("switcher", {
                action: "close"
            });
            session.command("capture", {
                action: "close"
            });
        }
    }
}
