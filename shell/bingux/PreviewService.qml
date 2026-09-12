pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

Singleton {
    id: root
    // One persistent broker shares prepared previews across result delegates.
    // Size and freshness checks stay in the worker, including cache hits.
    property int serial: 0
    property bool ready: false
    property var pending: ({})
    property var preloadPaths: []
    signal response(string requestId, var data)
    function request(args) {
        const id = String(++serial);
        const message = {
            op: "request",
            id: id,
            args: args
        };
        pending = Object.assign({}, pending, {
            [id]: message
        });
        if (ready)
            worker.write(JSON.stringify(message) + "\n");
        return id;
    }
    function cancel(id) {
        if (!id || !pending[id])
            return;
        const next = Object.assign({}, pending);
        delete next[id];
        pending = next;
        if (ready)
            worker.write(JSON.stringify({
                op: "cancel",
                id: id
            }) + "\n");
    }
    function preload(paths) {
        paths = BinguxPreferences.data.previews.prewarm && BinguxPreferences.data.previews.enabled ? paths : [];
        preloadPaths = paths;
        if (ready)
            worker.write(JSON.stringify({
                op: "preload",
                paths: paths
            }) + "\n");
    }
    Connections {
        target: BinguxPreferences
        function onDataChanged() {
            root.preload(root.preloadPaths);
        }
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_PREVIEW_HELPER") ? [Quickshell.env("BINGUX_PREVIEW_HELPER"), "serve"] : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("preview-document.py").toString().replace(/^file:\/\//, "")), "serve"]
        stdinEnabled: true
        running: true
        stdout: SplitParser {
            onRead: line => {
                let record;
                try {
                    record = JSON.parse(line);
                } catch (_) {
                    return;
                }
                if (record.ready) {
                    root.ready = true;
                    for (const message of Object.values(root.pending))
                        worker.write(JSON.stringify(message) + "\n");
                    worker.write(JSON.stringify({
                        op: "preload",
                        paths: root.preloadPaths
                    }) + "\n");
                } else if (root.pending[record.id]) {
                    const next = Object.assign({}, root.pending);
                    delete next[record.id];
                    root.pending = next;
                    root.response(record.id, record.data);
                }
            }
        }
        onExited: {
            root.ready = false;
            const requests = Object.keys(root.pending);
            root.pending = ({});
            for (const id of requests)
                root.response(id, {
                    error: "The preview service stopped. Try this file again."
                });
            restart.restart();
        }
    }
    Timer {
        id: restart
        interval: 500
        onTriggered: worker.running = true
    }
}
