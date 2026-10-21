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

    Component.onCompleted: {
        Quickshell.watchFiles = false;
        SessionContext.enter("greeter");
    }

    // Keyboard layouts for the session bar; the greeter session runs its own
    // bingux-statusd (see packaging/gnoblin/greeter.lua).
    Metrics {
        id: sessionMetrics
    }

    GreeterAuthentication {
        id: greeterAuthentication
    }

    GreeterCatalog {
        id: greeterCatalog
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: window
            required property var modelData
            readonly property bool primary: modelData === Quickshell.screens[0]
            screen: modelData
            // The wallpaper layer in GreeterScreen is the background.
            color: "transparent"
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
            // Variants instantiates this window as its own component, where
            // the screen's property names shadow same-named ids of this file.
            // The ids above are therefore distinct from the property names.
            GreeterScreen {
                anchors.fill: parent
                authentication: greeterAuthentication
                catalog: greeterCatalog
                metrics: sessionMetrics
                interactive: window.primary
            }
        }
    }
}
