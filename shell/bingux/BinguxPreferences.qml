pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({"search": {"disabledProviders": [], "ai": null, "fileRoots": null, "defaultEngine": "duckduckgo", "engines": [{"id": "duckduckgo", "name": "DuckDuckGo", "shortcut": "ddg", "url": "https://duckduckgo.com/?q={query}", "enabled": true}]}, "previews": {"enabled": true, "prewarm": true, "maxMegabytes": 20}, "desktop": {"dock": true, "sidebar": true, "metrics": true, "layout": null, "dockSize": 56, "dockAlignment": "center", "dockClick": "toggle", "dockMiddleClick": "launch", "dockScroll": "cycle", "dockScrollDirection": "natural", "sidebarEdge": null}})
    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/settings.json"
    readonly property var helper: Quickshell.env("BINGUX_SETTINGS_HELPER") ? [Quickshell.env("BINGUX_SETTINGS_HELPER")] : ["python3", decodeURIComponent(Qt.resolvedUrl("settings-backend.py").toString().replace(/^file:\/\//, ""))]
    property var pendingLayout: null
    function saveLayout(layout) {
        pendingLayout = layout;
        data = Object.assign({}, data, {desktop: Object.assign({}, data.desktop, {layout})});
        if (!writer.running) writer.running = true;
    }
    Process {
        id: writer
        command: root.helper.concat(["save"])
        stdinEnabled: true
        property var submitted
        onStarted: { submitted = root.pendingLayout; root.pendingLayout = null; write(JSON.stringify({desktop: {layout: submitted}})); stdinEnabled = false; }
        onExited: { stdinEnabled = true; if (root.pendingLayout) Qt.callLater(() => writer.running = true); }
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
        onFileChanged: reload()
        onLoaded: {
            try {
                const incoming = JSON.parse(text());
                root.data = {search: Object.assign({}, root.data.search, incoming.search || {}),
                    previews: Object.assign({}, root.data.previews, incoming.previews || {}),
                    desktop: Object.assign({}, root.data.desktop, incoming.desktop || {})};
            } catch (_) {}
        }
    }
}
