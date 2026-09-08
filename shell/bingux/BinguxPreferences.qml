pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({search: {disabledProviders: [], ai: null}, previews: {enabled: true, prewarm: true, maxMegabytes: 20}, desktop: {dock: true, sidebar: true, metrics: true}})
    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/settings.json"
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
