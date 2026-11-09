//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import "."

// Renders GreeterScreen after a rejected password, with the authentication and
// account catalog stubbed. The real status line (objectName greeterStatus)
// shows the failure text in the error colour.
ShellRoot {
    id: root

    Component.onCompleted: SessionContext.enter("greeter")

    Metrics {
        id: sessionMetrics
    }

    QtObject {
        id: stubAuthentication
        property string userName: "kieran"
        property var session: null
        readonly property bool available: true
        readonly property bool active: true
        readonly property bool finishing: false
        readonly property bool responseRequired: true
        property bool responseVisible: false
        readonly property string prompt: "Password:"
        readonly property string status: "Incorrect password, try again."
        readonly property bool statusIsError: true
        readonly property string hint: "Enter your password"
        readonly property bool fingerprintAvailable: false
        readonly property bool faceAvailable: false
        function respond(text) {
        }
        function start() {
        }
        function cancel() {
        }
        function finish() {
        }
    }

    QtObject {
        id: stubCatalog
        readonly property var users: [
            {
                name: "kieran",
                uid: 1000,
                userName: "kieran",
                displayName: "Kieran",
                icon: "",
                wallpaper: "",
                avatar: "",
                appearance: "",
                availability: "available"
            }
        ]
        readonly property var sessions: [
            {
                id: "bingux",
                name: "Bingux"
            }
        ]
        readonly property string rememberedUser: "kieran"
        readonly property string rememberedSession: "bingux"
        readonly property bool loaded: true
        readonly property int pending: 0
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
            WlrLayershell.namespace: "bingux-greeter-states-test"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            GreeterScreen {
                anchors.fill: parent
                authentication: stubAuthentication
                catalog: stubCatalog
                metrics: sessionMetrics
                hostWindow: window
                interactive: window.primary
                selectedUser: "kieran"
            }
        }
    }
}
