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
    readonly property bool busy: transitioning || request.running
    readonly property var actions: [
        {id: "suspend", title: "Suspend", icon: "weather-clear-night-symbolic"},
        {id: "logout", title: "Log Out", icon: "system-log-out-symbolic"},
        {id: "reboot", title: "Restart", icon: "system-reboot-symbolic"},
        {id: "poweroff", title: "Shut Down", icon: "system-shutdown-symbolic"}
    ]
    readonly property var selected: actions.find(action => action.id === pending)
    spacing: 4
    onVisibleChanged: if (!visible && !busy) { pending = ""; error = ""; }
    function execute() {
        if (!selected || busy) return;
        error = "";
        request.command = pending === "logout"
            ? ["gnome-session-quit", "--logout", "--no-prompt"]
            : ["systemctl", pending];
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
        NumberAnimation { target: root; property: "blackout"; to: 1; duration: Theme.reducedMotion ? 0 : 300; easing.type: Easing.InOutCubic }
        // Leave a frame at full black before the session starts to disappear.
        PauseAnimation { duration: 50 }
        ScriptAction { script: { request.running = true; recovery.restart(); } }
    }
    SequentialAnimation {
        id: fadeIn
        NumberAnimation { target: root; property: "blackout"; to: 0; duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
        ScriptAction { script: root.transitioning = false }
    }
    // A cancelled or inhibited session exit must not leave an opaque screen.
    Timer { id: recovery; interval: 10000; onTriggered: root.restore() }
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            visible: root.transitioning
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-session-fade"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors { top: true; bottom: true; left: true; right: true }
            // Let session inhibitor and authentication dialogs remain usable.
            mask: Region {}
            Rectangle { anchors.fill: parent; color: "black"; opacity: root.blackout }
        }
    }
    Process {
        id: request
        stderr: StdioCollector { onStreamFinished: root.error = text.trim(); }
        onExited: code => {
            if (code !== 0 && !root.error) root.error = "The session could not complete this request.";
            if (code !== 0) root.restore();
            // systemctl can return before sleep begins. Keep black briefly;
            // the timer also releases the overlay when the machine wakes.
            if (code === 0 && root.pending === "suspend") { recovery.interval = 1500; recovery.restart(); }
            if (code === 0) root.pending = "";
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
            onClicked: { root.error = ""; root.pending = modelData.id; }
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
        text: root.pending === "suspend" ? "Your session will remain open." : "Save your work before continuing."
        wrapMode: Text.Wrap
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
        onClicked: { root.pending = ""; root.error = ""; }
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
