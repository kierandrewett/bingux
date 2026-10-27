//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import "greeter"

// The bsm greeter and lock screen: one Quickshell client that bsm shows at
// boot, after logout, while a session starts and whenever a session is locked.
//
// It is a layer-shell client of bsm's greeter socket with one fullscreen
// Overlay window per output. It owns no xdg toplevel and no session lock: bsm
// itself keeps every desktop covered. It reads only the shared QML of this
// install and the sanitised images bsm published; nothing a user supplied is
// loaded or run. It talks to bsm over the control socket named by
// BSM_CONTROL_SOCKET (default /run/bsm/greeter-control.sock).
//
// This entry file lives at the root of the shell tree, not in greeter/ with
// its parts: Quickshell resolves imports only inside the folder of the entry
// file, and the parts share Theme and the auth/ components with the rest.
ShellRoot {
    id: root

    // Quickshell.env gives null for an unset variable, not "".
    readonly property string controlSocket: {
        const configured = Quickshell.env("BSM_CONTROL_SOCKET");
        return typeof configured === "string" && configured !== "" ? configured : "/run/bsm/greeter-control.sock";
    }

    Component.onCompleted: {
        Quickshell.watchFiles = false;
        SessionContext.enter("greeter");
        greeterClient.start();
    }

    BsmGreeterModel {
        id: greeterModel
    }

    BsmAuthClient {
        id: greeterClient
        model: greeterModel
        path: root.controlSocket
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: window
            required property var modelData
            readonly property bool primary: modelData === Quickshell.screens[0]
            screen: modelData
            // The wallpaper layer of the screen is the background.
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "bingux-bsm-greeter"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: primary ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            // Variants instantiates this window as its own component, where
            // the screen's property names shadow same-named ids of this file.
            BsmGreeterScreen {
                anchors.fill: parent
                model: greeterModel
                client: greeterClient
                interactive: window.primary
            }
        }
    }
}
