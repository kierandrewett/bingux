import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ColumnLayout {
    id: root
    // The action waiting for confirmation, and the one already asked for. They
    // differ when Cancel closes the confirm step while the request still runs.
    property string pending: ""
    property string inFlight: ""
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
            icon: "system-log-out-symbolic",
            destructive: true
        },
        {
            id: "reboot",
            title: "Restart",
            icon: "system-reboot-symbolic",
            destructive: true
        },
        {
            id: "poweroff",
            title: "Shut Down",
            icon: "system-shutdown-symbolic",
            destructive: true
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
        inFlight = pending;
        if (integrated) {
            // No gnoblinctl and no local fade: bsm runs the transition. The
            // request is only followed here; success is whatever bsm reports.
            if (!BsmSession.request(pending)) {
                inFlight = "";
                error = "Power options are not available. Try again in a moment.";
            }
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
    // Closes the confirm step. Cancel stays available while a request runs, so
    // nothing here is ever a dead end; before the request has started the fade
    // out is undone, after that the request is left to finish.
    function cancel() {
        if (!integrated && transitioning && !request.running && fadeOut.running) {
            fadeOut.stop();
            inFlight = "";
            restore();
        }
        pending = "";
        error = "";
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
            onStreamFinished: if (text.trim())
                console.warn("session: gnoblinctl said: " + text.trim())
        }
        onExited: code => {
            if (code !== 0 && !root.error)
                root.error = "Could not complete this request. Try again.";
            if (code !== 0)
                root.restore();
            // systemctl can return before sleep begins. Keep black briefly;
            // the timer also releases the overlay when the machine wakes.
            if (code === 0 && root.inFlight === "suspend") {
                recovery.interval = 1500;
                recovery.restart();
            }
            if (code === 0) {
                root.pending = "";
                root.inFlight = "";
            }
        }
    }
    // Integrated mode: the end of the operation this view asked for. Only
    // bsm's own report counts; a request that was merely accepted does not.
    Connections {
        target: BsmSession
        enabled: root.integrated
        function onFinished(operation) {
            if (operation.action !== root.inFlight)
                return;
            root.error = operation.status === "completed" ? "" : BsmSession.failureText;
            root.pending = "";
            root.inFlight = "";
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
        text: BsmSession.phaseLabel + "…"
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
    // The confirm step: the action, toned as destructive when it ends the
    // session, and Cancel with a close glyph. Only the action waits for a
    // request in progress; Cancel never greys out.
    ControlRow {
        objectName: "powerConfirm"
        Layout.fillWidth: true
        visible: root.pending !== ""
        title: root.selected ? root.selected.title : ""
        iconName: root.selected ? root.selected.icon : ""
        destructive: !!root.selected && root.selected.destructive === true
        enabled: !root.busy
        onClicked: root.execute()
    }
    ControlRow {
        objectName: "powerCancel"
        Layout.fillWidth: true
        visible: root.pending !== ""
        title: "Cancel"
        iconName: "window-close-symbolic"
        onClicked: root.cancel()
    }
    StateMessage {
        objectName: "powerError"
        compact: true
        visible: root.error !== ""
        tone: "danger"
        title: root.error
    }
}
