pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property bool active: false
    property bool statusVisible: false
    readonly property bool monitoring: active || statusVisible
    property bool ready: false
    property bool busy: false
    property string error: ""
    property var state: ({vpns: [], power: {available: false, profiles: []}})
    readonly property var vpns: state.vpns || []
    readonly property bool doNotDisturb: !!state.doNotDisturb
    property bool keepAwake: false
    property var controls: ({vpn: true, dnd: true, nightLight: false, power: false, awake: false})
    property bool preferencesReady: false
    readonly property string preferencesDirectory: Quickshell.statePath("control-centre")
    readonly property var effectiveControls: DesktopEditing.desktop.controlCentre || controls
    function showControl(name) { return effectiveControls[name] === true; }
    function setControl(name, shown) {
        if (!(name in controls)) return;
        if (BinguxPreferences.data.desktop.controlCentre) {
            BinguxPreferences.saveDesktop({controlCentre: Object.assign({}, effectiveControls, {[name]: shown})});
            return;
        }
        controls = Object.assign({}, controls, {[name]: shown});
        if (preferencesReady) preferences.setText(JSON.stringify(controls));
    }
    function action(request) {
        if (!ready || busy) return;
        busy = true;
        error = "";
        worker.write(JSON.stringify(Object.assign({op: "action"}, request)) + "\n");
    }
    function toggleAwake() {
        error = "";
        if (state.awakeAvailable) keepAwake = !keepAwake;
    }
    onMonitoringChanged: if (ready) worker.write(JSON.stringify({op: "active", value: monitoring}) + "\n")
    Process {
        id: directory
        command: ["mkdir", "-p", "-m", "700", root.preferencesDirectory]
        running: true
        onExited: code => { if (code === 0) preferences.path = root.preferencesDirectory + "/controls.json"; }
    }
    FileView {
        id: preferences
        atomicWrites: true
        printErrors: false
        onLoaded: {
            try {
                const saved = JSON.parse(text());
                const next = Object.assign({}, root.controls);
                for (const name of Object.keys(next)) if (typeof saved[name] === "boolean") next[name] = saved[name];
                root.controls = next;
            } catch (_) {}
            root.preferencesReady = true;
        }
        onLoadFailed: root.preferencesReady = true
        onSaveFailed: root.error = "Could not save control preferences."
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_CONTROLS_HELPER") ? [Quickshell.env("BINGUX_CONTROLS_HELPER")]
            : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("control-centre-services.py").toString().replace(/^file:\/\//, ""))]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: line => {
                let response;
                try { response = JSON.parse(line); } catch (_) { return; }
                if (response.state) root.state = Object.assign({}, root.state, response.state);
                if (response.ready) {
                    root.ready = true;
                    worker.write(JSON.stringify({op: "active", value: root.monitoring}) + "\n");
                }
                if (response.actionDone) root.busy = false;
                if (response.error) root.error = response.error;
            }
        }
        onExited: {
            root.ready = false;
            root.busy = false;
            root.error = "Desktop controls are unavailable.";
            restart.restart();
        }
    }
    Timer { id: restart; interval: 5000; onTriggered: worker.running = true }
    Process {
        id: inhibitor
        command: [Quickshell.env("BINGUX_SESSION_INHIBIT") || "gnome-session-inhibit", "--inhibit", "idle:suspend", "--reason", "Keep Awake from Control Centre", "sleep", "infinity"]
        running: root.keepAwake
        onExited: code => {
            if (root.keepAwake) {
                root.keepAwake = false;
                root.error = "Keep Awake could not be started.";
            }
        }
    }
}
