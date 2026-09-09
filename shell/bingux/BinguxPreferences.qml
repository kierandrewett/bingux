pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({"search": {"disabledProviders": [], "ai": null, "fileRoots": null, "defaultEngine": "duckduckgo", "engines": [{"id": "duckduckgo", "name": "DuckDuckGo", "shortcut": "ddg", "url": "https://duckduckgo.com/?q={query}", "enabled": true}]}, "previews": {"enabled": true, "prewarm": true, "maxMegabytes": 20}, "desktop": {"dock": true, "sidebar": true, "metrics": true, "layout": null, "dockSize": 56, "dockAlignment": "center", "dockClick": "toggle", "dockMiddleClick": "launch", "dockScroll": "cycle", "dockScrollDirection": "natural", "sidebarEdge": null, "layoutVersion": 0, "dockApps": null, "controlCentre": null, "controlLayout": null, "containers": {}, "widgetOptions": {}}})
    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/settings.json"
    readonly property var helper: Quickshell.env("BINGUX_SETTINGS_HELPER") ? [Quickshell.env("BINGUX_SETTINGS_HELPER")] : ["python3", decodeURIComponent(Qt.resolvedUrl("settings-backend.py").toString().replace(/^file:\/\//, ""))]
    property bool loaded: false
    property string layoutError: ""
    property var pendingDesktop: null
    property int readRevision: 0
    function reload() {
        readRevision++;
        if (reader.running) reader.requested = true;
        else reader.running = true;
    }
    function saveLayout(layout) { saveDesktop({layout}); }
    function saveDesktop(changes) {
        pendingDesktop = Object.assign({}, pendingDesktop || {}, changes);
        data = Object.assign({}, data, {desktop: Object.assign({}, data.desktop, changes)});
        if (!writer.running) writer.running = true;
    }
    function importDesktop(snapshot) {
        if (importer.running || (data.desktop.layoutVersion === 1 && data.desktop.controlLayout)) return;
        importer.snapshot = snapshot;
        importer.running = true;
    }
    Process {
        id: importer
        command: root.helper.concat(["import-layout"])
        property var snapshot
        stdinEnabled: true
        onStarted: { write(JSON.stringify(snapshot)); stdinEnabled = false; }
        onExited: stdinEnabled = true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (result.error) root.layoutError = result.error;
                    else { root.data = result.data; root.layoutError = ""; file.reload(); }
                } catch (_) { root.layoutError = "Could not import the current desktop layout."; }
            }
        }
    }
    Process {
        id: writer
        command: root.helper.concat(["save"])
        stdinEnabled: true
        property var submitted
        onStarted: { submitted = root.pendingDesktop; root.pendingDesktop = null; write(JSON.stringify({desktop: submitted})); stdinEnabled = false; }
        onExited: { stdinEnabled = true; if (root.pendingDesktop) Qt.callLater(() => writer.running = true); else file.reload(); }
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (result.error) { console.warn("Could not save layout:", result.error); file.reload(); }
                } catch (_) { console.warn("Could not save layout"); file.reload(); }
            }
        }
    }
    Process {
        id: reader
        command: root.helper.concat(["read"])
        property bool requested: false
        property int revision: 0
        onStarted: { requested = false; revision = root.readRevision; }
        onExited: if (requested) Qt.callLater(() => reader.running = true)
        stdout: StdioCollector {
            onStreamFinished: {
                if (reader.revision !== root.readRevision) return;
                try {
                    const result = JSON.parse(text);
                    if (result.error) throw new Error(result.error);
                    const incoming = result.data;
                    if (root.data.desktop.layoutVersion === 1 && incoming.desktop.layoutVersion !== 1)
                        throw new Error("The desktop layout has changed. Restore a version 1 layout.");
                    root.data = {search: incoming.search, previews: incoming.previews,
                        desktop: Object.assign({}, incoming.desktop, writer.running ? writer.submitted || {} : {}, root.pendingDesktop || {})};
                    root.layoutError = "";
                } catch (error) { root.layoutError = error.message || "Could not load desktop settings."; }
                root.loaded = true;
            }
        }
    }
    FileView {
        id: file
        path: root.path
        watchChanges: true
        printErrors: false
        onFileChanged: root.reload()
        onLoadFailed: root.reload()
        onLoaded: root.reload()
    }
}
