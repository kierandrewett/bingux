pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root
    readonly property int apiVersion: 1
    property var extensions: []
    property var widgets: []
    property var errors: []
    property var internals: null
    property int generation: 0
    property bool anyEnabled: true
    Component.onCompleted: {
        try {
            let text = "";
            try {
                text = settingsFile.text();
            } catch (_) {}
            anyEnabled = text.trim() ? JSON.parse(text).enabled.length > 0 : false;
        } catch (_) {
            anyEnabled = true;
        }
    }
    property var actions: ({})
    signal event(string name, var payload)
    function invoke(id, payload) {
        const action = actions[id];
        if (!action)
            throw new Error("Unknown extension action: " + id);
        return action(payload);
    }
    function setEnabled(id, enabled) {
        if (manager.running)
            return;
        manager.command = ["python3", decodeURIComponent(Qt.resolvedUrl("extensions.py").toString().replace("file://", "")), enabled ? "enable" : "disable", id];
        manager.running = true;
    }
    Process {
        id: manager
        onExited: (code, status) => {
            if (code !== 0)
                root.report("manager", "Could not change extension state");
            root.reload();
        }
    }
    function widget(id) {
        return widgets.find(item => item.id === id) || null;
    }
    function reload() {
        if (!scan.running)
            scan.running = true;
    }
    function report(id, message) {
        errors = errors.filter(item => item.id !== id).concat([
            {
                id,
                message
            }
        ]);
        console.warn("[extensions] " + id + ": " + message);
    }
    Process {
        id: scan
        command: ["python3", decodeURIComponent(Qt.resolvedUrl("extensions.py").toString().replace("file://", "")), "list"]
        // With nothing enabled there are no widgets to place, so the scan can
        // wait until the desktop is up instead of delaying start-up.
        // The lock screen and the greeter never load extensions: an
        // extension sees the whole shell through `unstable`.
        running: !SessionContext.restricted && (root.anyEnabled || Startup.settled)
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const data = JSON.parse(text);
                    root.extensions = data.extensions;
                    root.widgets = data.widgets;
                    root.errors = data.errors;
                    root.generation++;
                } catch (error) {
                    root.report("loader", "Could not read extension registry: " + error);
                }
            }
        }
    }
    FileView {
        id: settingsFile
        blockLoading: true
        path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/extensions.json"
        watchChanges: true
        onFileChanged: root.reload()
    }
}
