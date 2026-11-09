//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import "."

// Renders the lock screen's SessionScreen in the locked context, outside a
// real session lock, with a stubbed authentication. The session bar
// (objectName sessionBar) is the locked-down StatusBar shown at the top.
ShellRoot {
    id: root

    Component.onCompleted: SessionContext.enter("locked")

    Metrics {
        id: sessionMetrics
    }

    QtObject {
        id: stubAuthentication
        property string userName: "kieran"
        property string userDisplayName: "Kieran"
        property url userImage: ""
        property url wallpaper: ""
        readonly property bool active: true
        readonly property bool responseRequired: true
        readonly property bool responseVisible: false
        readonly property string prompt: "Password:"
        property string status: ""
        property bool statusIsError: false
        readonly property bool finishing: false
        readonly property string hint: "Enter your password to unlock"
        readonly property bool fingerprintAvailable: false
        readonly property bool faceAvailable: false
        signal responseRequested
        function start() {
        }
        function respond(text) {
        }
        function finish() {
        }
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
            WlrLayershell.namespace: "bingux-lock-states-test"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            SessionScreen {
                anchors.fill: parent
                authentication: stubAuthentication
                metrics: sessionMetrics
                hostWindow: window
                interactive: window.primary
                ready: true
                accountName: "kieran"
                accountDisplayName: "Kieran"
            }
        }
    }
}
