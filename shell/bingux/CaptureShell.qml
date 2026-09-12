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
        sessionName: "capture"
        bindings: [
            {
                id: "capture",
                accelerator: Quickshell.env("BINGUX_CAPTURE_SHORTCUT") || "<Alt>s",
                hold: 8,
                modal: false
            }
        ]
        onActivated: (id, first) => {
            if (first) {
                capture.open();
            }
        }
        state: ({
                visible: capture.opened,
                surface: "bingux-capture",
                companions: ["bingux-capture-controls"],
                companionsAbove: true,
                state: capture.state,
                busy: capture.busy,
                recording: capture.recording,
                elapsedText: capture.elapsedText,
                countdown: capture.countdown
            })
        onCommandReceived: command => {
            if (command.action === "open")
                capture.open();
            else if (command.action === "close")
                capture.close();
            else if (command.action === "stop")
                capture.stop();
        }
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
        onOpening: {
            session.command("search", {
                action: "close"
            });
            session.command("switcher", {
                action: "close"
            });
            session.command("emoji", {
                action: "close"
            });
        }
    }
}
