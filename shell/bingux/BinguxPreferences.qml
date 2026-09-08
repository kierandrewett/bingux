pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({"search": {"disabledProviders": [], "ai": null, "fileRoots": null, "defaultEngine": "duckduckgo", "engines": [{"id": "duckduckgo", "name": "DuckDuckGo", "shortcut": "ddg", "url": "https://duckduckgo.com/?q={query}", "enabled": true}]}, "previews": {"enabled": true, "prewarm": true, "maxMegabytes": 20}, "desktop": {"dock": true, "sidebar": true, "metrics": true, "layout": null, "dockSize": 56, "dockAlignment": "center", "dockClick": "toggle", "dockMiddleClick": "launch", "dockScroll": "cycle", "dockScrollDirection": "natural", "sidebarEdge": null, "layoutVersion": 0, "dockApps": null, "controlCentre": null, "containers": {}, "widgetOptions": {}}})
    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/settings.json"
    readonly property var helper: Quickshell.env("BINGUX_SETTINGS_HELPER") ? [Quickshell.env("BINGUX_SETTINGS_HELPER")] : ["python3", decodeURIComponent(Qt.resolvedUrl("settings-backend.py").toString().replace(/^file:\/\//, ""))]
    property bool loaded: false
    property string layoutError: ""
    property var pendingDesktop: null
    function saveLayout(layout) { saveDesktop({layout}); }
    function saveDesktop(changes) {
        pendingDesktop = Object.assign({}, pendingDesktop || {}, changes);
        data = Object.assign({}, data, {desktop: Object.assign({}, data.desktop, changes)});
        if (!writer.running) writer.running = true;
    }
    function importDesktop(snapshot) {
        if (importer.running || data.desktop.layoutVersion === 1) return;
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
                    else { root.data = result.data; root.layoutError = ""; }
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
    FileView {
        id: file
        path: root.path
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoadFailed: root.loaded = true
        onLoaded: {
            try {
                const incoming = JSON.parse(text());
                if (![0, 1].includes(incoming.desktop?.layoutVersion || 0)) throw new Error("This desktop layout version is not supported.");
                root.data = {search: Object.assign({}, root.data.search, incoming.search || {}),
                    previews: Object.assign({}, root.data.previews, incoming.previews || {}),
                    desktop: Object.assign({}, root.data.desktop, incoming.desktop || {}, writer.running ? writer.submitted || {} : {}, root.pendingDesktop || {})};
                root.layoutError = "";
            } catch (error) { root.layoutError = error.message; }
            root.loaded = true;
        }
    }
}
