import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ColumnLayout {
    id: root
    property string pending: ""
    property string error: ""
    property bool transitioning: false
    property real blackout: 0
    // Integrated mode (BSM_SESSION_ID set): bsm owns the action, its
    // authorisation and every transition. This view then only asks, shows the
    // confirmation and any error, and follows bsm's reported outcome.
    readonly property bool integrated: BsmMode.integrated
    readonly property bool busy: integrated ? BsmSession.busy : (transitioning || request.running)
    readonly property var actions: integrated ? baseActions.concat([switchUserAction]) : baseActions
    readonly property var switchUserAction: ({
            id: "switch-user",
            title: "Switch User",
            icon: "system-switch-user-symbolic"
        })
    readonly property var baseActions: [
        {
            id: "suspend",
            title: "Suspend",
            icon: "weather-clear-night-symbolic"
        },
        {
            id: "logout",
            title: "Log Out",
            icon: "system-log-out-symbolic"
        },
        {
            id: "reboot",
            title: "Restart",
            icon: "system-reboot-symbolic"
        },
        {
            id: "poweroff",
            title: "Shut Down",
            icon: "system-shutdown-symbolic"
        }
    ]
    readonly property var selected: actions.find(action => action.id === pending)
    spacing: 4
    onVisibleChanged: if (!visible && !busy) {
        pending = "";
        error = "";
    }
    function execute() {
        if (!selected || busy || !SessionServices.allows("power"))
            return;
        error = "";
        if (integrated) {
            // No gnoblinctl and no local fade: bsm runs the transition. The
            // request is only followed here; success is whatever bsm reports.
            if (!BsmSession.request(pending))
                error = "The session manager is not available.";
            return;
        }
        request.command = ["gnoblinctl", "session", pending];
        recovery.interval = 10000;
        transitioning = true;
        fadeOut.start();
    }
    function restore() {
        recovery.stop();
        fadeIn.start();
    }
    SequentialAnimation {
        id: fadeOut
        NumberAnimation {
            target: root
            property: "blackout"
            to: 1
            duration: Theme.reducedMotion ? 0 : 300
            easing.type: Easing.InOutCubic
        }
        // Leave a frame at full black before the session starts to disappear.
        PauseAnimation {
            duration: 50
        }
        ScriptAction {
            script: {
                request.running = true;
                recovery.restart();
            }
        }
    }
    SequentialAnimation {
        id: fadeIn
        NumberAnimation {
            target: root
            property: "blackout"
            to: 0
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
        }
        ScriptAction {
            script: root.transitioning = false
        }
    }
    // A cancelled or inhibited session exit must not leave an opaque screen.
    Timer {
        id: recovery
        interval: 10000
        onTriggered: root.restore()
    }
    Variants {
        // The black overlay belongs to the standalone path only: in integrated
        // mode bsm composes every full-screen fade.
        model: root.integrated ? [] : Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            visible: root.transitioning
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-session-fade"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            // Let session inhibitor and authentication dialogs remain usable.
            mask: Region {}
            Rectangle {
                anchors.fill: parent
                color: "black"
                opacity: root.blackout
            }
        }
    }
    Process {
        id: request
        stderr: StdioCollector {
            onStreamFinished: root.error = text.trim()
        }
        onExited: code => {
            if (code !== 0 && !root.error)
                root.error = "The session could not complete this request.";
            if (code !== 0)
                root.restore();
            // systemctl can return before sleep begins. Keep black briefly;
            // the timer also releases the overlay when the machine wakes.
            if (code === 0 && root.pending === "suspend") {
                recovery.interval = 1500;
                recovery.restart();
            }
            if (code === 0)
                root.pending = "";
        }
    }
    // Integrated mode: the end of the operation this view asked for. Only
    // bsm's own report counts; a request that was merely accepted does not.
    Connections {
        target: BsmSession
        enabled: root.integrated
        function onFinished(operation) {
            if (operation.action !== root.pending)
                return;
            root.error = operation.status === "completed" ? "" : BsmSession.failureText;
            root.pending = "";
            BsmSession.dismiss();
        }
    }
    Repeater {
        model: root.pending ? [] : root.actions
        ControlRow {
            required property var modelData
            Layout.fillWidth: true
            title: modelData.title
            iconName: modelData.icon
            navigation: true
            onClicked: {
                root.error = "";
                root.pending = modelData.id;
            }
        }
    }
    Text {
        Layout.fillWidth: true
        visible: root.pending !== ""
        text: root.selected ? root.selected.title + "?" : ""
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontHeading
        font.weight: Font.Medium
    }
    Text {
        Layout.fillWidth: true
        visible: root.pending !== ""
        text: root.pending === "suspend" || root.pending === "switch-user" ? "Your session will remain open." : "Save your work before continuing."
        wrapMode: Text.Wrap
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    Text {
        Layout.fillWidth: true
        visible: root.integrated && root.busy
        text: BsmSession.phaseLabel + "..."
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    ControlRow {
        Layout.fillWidth: true
        visible: root.pending !== ""
        title: root.selected ? root.selected.title : ""
        iconName: root.selected ? root.selected.icon : ""
        enabled: !root.busy
        onClicked: root.execute()
    }
    ControlRow {
        Layout.fillWidth: true
        visible: root.pending !== ""
        title: "Cancel"
        iconName: "go-previous-symbolic"
        enabled: !root.busy
        onClicked: {
            root.pending = "";
            root.error = "";
        }
    }
    Text {
        Layout.fillWidth: true
        visible: root.error !== ""
        text: root.error
        wrapMode: Text.Wrap
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
}
