pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    property var data: ({
            "search": {
                "disabledProviders": [],
                "ai": null,
                "fileRoots": null,
                "defaultEngine": "duckduckgo",
                "engines": [
                    {
                        "id": "duckduckgo",
                        "name": "DuckDuckGo",
                        "shortcut": "ddg",
                        "url": "https://duckduckgo.com/?q={query}",
                        "enabled": true
                    }
                ]
            },
            "previews": {
                "enabled": true,
                "prewarm": true,
                "maxMegabytes": 20
            },
            "appearance": {
                "fontFamily": ""
            },
            "desktop": {
                "dock": true,
                "sidebar": true,
                "metrics": true,
                "clockSeconds": false,
                "wallpaper": "",
                "layout": null,
                "dockSize": 56,
                "dockAlignment": "center",
                "dockClick": "toggle",
                "dockMiddleClick": "launch",
                "dockScroll": "cycle",
                "dockScrollDirection": "natural",
                "sidebarEdge": null,
                "layoutVersion": 0,
                "dockApps": null,
                "dockFolders": [],
                "controlCentre": null,
                "controlOrder": null,
                "controlLayout": null,
                "containers": {},
                "widgetOptions": {}
            }
        })
    readonly property string path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/bingux.toml"
    // bingux-config owns the schema, validation and the TOML file. The shell
    // only speaks its JSON protocol, so manual edits and graphical edits share
    // one set of rules.
    readonly property var helper: [Python.config]
    property int wallpaperRevision: 0
    property string wallpaperUrl: ""
    readonly property string wallpaperPath: data.desktop.wallpaper || ""
    property bool loaded: false
    property string layoutError: ""
    property var pendingDesktop: null
    property int readRevision: 0
    function reload() {
        readRevision++;
        if (reader.running)
            reader.requested = true;
        else
            reader.running = true;
    }
    // The first read happens at once: the binary starts in milliseconds and
    // the defaults above keep the shell usable until it replies.
    property string lastNotifiedError: ""
    function reportError(message) {
        layoutError = message;
        if (!loaded || message === lastNotifiedError)
            return;
        lastNotifiedError = message;
        ShellNotifications.send("Settings file has a problem", message + " Run `bingux-config check` after editing " + path + ".");
    }
    function reloadWallpaper() {
        wallpaperRevision++;
        if (!wallpaperPath) {
            wallpaperUrl = "";
            return;
        }
        // Formats Qt reads directly need no helper.
        if (wallpaperPath.startsWith("/") && /\.(png|jpe?g|webp)$/i.test(wallpaperPath)) {
            wallpaperUrl = "file://" + encodeURI(wallpaperPath).replace(/#/g, "%23").replace(/\?/g, "%3F");
            return;
        }
        if (wallpaperReader.running) {
            wallpaperReader.pending = true;
            return;
        }
        wallpaperReader.running = true;
    }
    Component.onCompleted: {
        reload();
        reloadWallpaper();
    }
    property bool verifyPending: false
    function verify() {
        reload();
    }
    onWallpaperPathChanged: reloadWallpaper()
    function saveLayout(layout) {
        saveDesktop({
            layout
        });
    }
    function saveDesktop(changes) {
        pendingDesktop = Object.assign({}, pendingDesktop || {}, changes);
        data = Object.assign({}, data, {
            desktop: Object.assign({}, data.desktop, changes)
        });
        if (!writer.running)
            writer.running = true;
    }
    function importDesktop(snapshot) {
        if (importer.running || (data.desktop.layoutVersion === 1 && data.desktop.controlLayout))
            return;
        importer.snapshot = snapshot;
        importer.running = true;
    }
    Process {
        id: importer
        command: root.helper.concat(["import-layout"])
        property var snapshot
        stdinEnabled: true
        onStarted: {
            write(JSON.stringify(snapshot));
            stdinEnabled = false;
        }
        onExited: stdinEnabled = true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (result.error)
                        root.layoutError = result.error;
                    else {
                        root.data = result.data;
                        root.layoutError = "";
                        file.reload();
                    }
                } catch (_) {
                    root.layoutError = "Could not import the current desktop layout.";
                }
            }
        }
    }
    Process {
        id: writer
        command: root.helper.concat(["save"])
        stdinEnabled: true
        property var submitted
        onStarted: {
            submitted = root.pendingDesktop;
            root.pendingDesktop = null;
            write(JSON.stringify({
                desktop: submitted
            }));
            stdinEnabled = false;
        }
        onExited: {
            stdinEnabled = true;
            if (root.pendingDesktop)
                Qt.callLater(() => writer.running = true);
            else
                file.reload();
        }
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (result.error) {
                        console.warn("Could not save layout:", result.error);
                        file.reload();
                    }
                } catch (_) {
                    console.warn("Could not save layout");
                    file.reload();
                }
            }
        }
    }
    Process {
        id: reader
        command: root.helper.concat(["read"])
        property bool requested: false
        property int revision: 0
        onStarted: {
            requested = false;
            revision = root.readRevision;
        }
        onExited: if (requested)
            Qt.callLater(() => reader.running = true)
        stdout: StdioCollector {
            onStreamFinished: {
                if (reader.revision !== root.readRevision)
                    return;
                try {
                    const result = JSON.parse(text);
                    if (result.error)
                        throw new Error(result.error);
                    const incoming = result.data;
                    if (root.data.desktop.layoutVersion === 1 && incoming.desktop.layoutVersion !== 1)
                        throw new Error("The desktop layout has changed. Restore a version 1 layout.");
                    const next = {
                        appearance: incoming.appearance,
                        search: incoming.search,
                        previews: incoming.previews,
                        desktop: Object.assign({}, incoming.desktop, writer.running ? writer.submitted || {} : {}, root.pendingDesktop || {})
                    };
                    // An unchanged read must not rebuild everything bound to the data.
                    if (JSON.stringify(next) !== JSON.stringify(root.data))
                        root.data = next;
                    root.layoutError = "";
                } catch (error) {
                    root.reportError(error.message || "Could not load desktop settings.");
                }
                root.loaded = true;
            }
        }
    }
    // Resolve once for every screen. The helper validates the image path and
    // converts formats Qt cannot read directly into a cached PNG.
    Process {
        id: wallpaperReader
        command: root.helper.concat(["wallpaper"])
        property bool pending: false
        property int revision: 0
        onStarted: revision = root.wallpaperRevision
        onExited: {
            if (pending) {
                pending = false;
                running = true;
            }
        }
        stdout: StdioCollector {
            onStreamFinished: {
                if (wallpaperReader.revision !== root.wallpaperRevision)
                    return;
                try {
                    root.wallpaperUrl = JSON.parse(text).wallpaper || "";
                } catch (_) {
                    root.wallpaperUrl = "";
                }
            }
        }
    }
    // Manual edits to the TOML file are picked up through the watcher; the
    // binary re-validates on every change.
    FileView {
        id: file
        path: root.path
        blockLoading: false
        watchChanges: true
        printErrors: false
        onFileChanged: root.reload()
    }
}
