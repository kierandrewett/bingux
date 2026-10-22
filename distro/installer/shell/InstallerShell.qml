//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland

// The Bingux installer. One layer surface per output; the first output carries
// the wizard and the keyboard, the others show the same background. The
// backend is bingux-installer-daemon (distro/installer/README.md).
ShellRoot {
    id: root

    Component.onCompleted: {
        Quickshell.watchFiles = false;
        SessionContext.enter("kiosk");
        client.probe();
    }

    Metrics {
        id: sessionMetrics
    }

    InstallerClient {
        id: client
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: window
            required property var modelData
            readonly property bool primary: modelData === Quickshell.screens[0]
            screen: modelData
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "bingux-installer"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: primary ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            // Variants instantiates this window as its own component, so the
            // screen's property names shadow same-named ids of this file.
            InstallerWizard {
                anchors.fill: parent
                metrics: sessionMetrics
                client: client
                hostWindow: window
                interactive: window.primary
            }
        }
    }
}
