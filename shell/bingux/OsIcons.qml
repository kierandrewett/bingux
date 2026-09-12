pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "IconCache.js" as IconCache

Singleton {
    id: root
    property var sources: ({})
    property var palettes: ({})
    property var requested: ({})
    property bool ready: false
    property bool unavailable: false
    property var cache: IconCache.create()
    property var cacheStats: IconCache.stats(cache)

    function retain(source) {
        IconCache.retain(cache, source);
        resolve(source);
        trim();
    }
    function release(source) {
        IconCache.release(cache, source);
        trim();
    }
    function trim() {
        const removed = IconCache.trim(cache);
        if (removed.length) {
            const nextSources = Object.assign({}, sources);
            const nextPalettes = Object.assign({}, palettes);
            const nextRequested = Object.assign({}, requested);
            for (const source of removed) {
                delete nextSources[source];
                delete nextPalettes[source];
                delete nextRequested[source];
            }
            sources = nextSources;
            palettes = nextPalettes;
            requested = nextRequested;
        }
        cacheStats = IconCache.stats(cache);
    }

    function resolve(source) {
        if (!source)
            return;
        IconCache.touch(cache, source);
        if (requested[source])
            return;
        requested = Object.assign({}, requested, {
            [source]: true
        });
        if (unavailable)
            sources = Object.assign({}, sources, {
                [source]: source
            });
        else if (ready)
            worker.write(JSON.stringify({
                source: source
            }) + "\n");
        trim();
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_ICON_HELPER") ? [Quickshell.env("BINGUX_ICON_HELPER")] : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("render-os-icons.py").toString().replace(/^file:\/\//, ""))]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: data => {
                let response;
                try {
                    response = JSON.parse(data);
                } catch (_) {
                    return;
                }
                if (response.ready || response.reset) {
                    root.ready = true;
                    if (response.reset) {
                        root.sources = ({});
                        root.palettes = ({});
                    }
                    for (const source of Object.keys(root.requested))
                        worker.write(JSON.stringify({
                            source: source
                        }) + "\n");
                } else if (typeof response.source === "string" && typeof response.resolved === "string") {
                    if (!root.requested[response.source])
                        return;
                    root.sources = Object.assign({}, root.sources, {
                        [response.source]: response.resolved
                    });
                    if (response.palette)
                        root.palettes = Object.assign({}, root.palettes, {
                            [response.source]: response.palette
                        });
                    IconCache.touch(root.cache, response.source, 2 * (response.source.length + response.resolved.length + JSON.stringify(response.palette || []).length));
                    root.trim();
                    if (response.error)
                        console.warn("OS icon fallback:", response.source, response.error);
                }
            }
        }
        onExited: {
            root.ready = false;
            root.unavailable = true;
            const fallback = Object.assign({}, root.sources);
            for (const source of Object.keys(root.requested))
                if (!fallback[source])
                    fallback[source] = source;
            root.sources = fallback;
            console.warn("OS icon renderer unavailable; using native Qt icons.");
        }
    }
}
