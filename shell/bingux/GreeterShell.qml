//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland

// The login greeter. greetd starts Gnoblin in greeter mode, Gnoblin autostarts
// this client with GREETD_SOCK, and greetd ends the whole greeter session once
// the chosen user session has been accepted. One layer surface covers every
// output; the first output carries the prompt and controls.
ShellRoot {
    id: root

    Component.onCompleted: Quickshell.watchFiles = false

    GreeterAuthentication {
        id: authentication
    }

    GreeterCatalog {
        id: catalog
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: window
            required property var modelData
            readonly property bool primary: modelData === Quickshell.screens[0]
            screen: modelData
            color: Theme.background
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "bingux-greeter"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: primary ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            GreeterScreen {
                anchors.fill: parent
                authentication: authentication
                catalog: catalog
                interactive: window.primary
            }
        }
    }
}
