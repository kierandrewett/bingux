pragma Singleton
import QtQuick
import QtQuick.Window
import Quickshell

Singleton {
    id: root
    property var editor: null
    readonly property bool active: !!editor && editor.visible
    readonly property var desktop: active ? Object.assign({}, editor.desktop, {layout: editor.layout}) : BinguxPreferences.data.desktop
    property var surfaces: []
    property var sources: ({})
    property var previews: ({})
    function registerSource(id, item) { sources = Object.assign({}, sources, {[id]: item}); }
    function unregisterSource(id, item) {
        if (sources[id] !== item) return;
        const next = Object.assign({}, sources); delete next[id]; sources = next;
    }
    function registerSurface(surface) { surfaces = surfaces.concat([surface]); }
    function unregisterSurface(surface) { surfaces = surfaces.filter(value => value !== surface); }
    function point(item, window, x, y) {
        const local = item.mapToItem(window.contentItem, x, y);
        return Qt.point(local.x + (window.anchors.right && !window.anchors.left ? window.screen.width - window.width - window.margins.right : window.margins.left),
            local.y + (window.anchors.bottom && !window.anchors.top ? window.screen.height - window.height - window.margins.bottom : window.margins.top));
    }
    property var pendingCaptures: []
    property bool capturing: false
    function capture() {
        if (capturing || !active) return;
        pendingCaptures = Object.keys(sources).filter(id => !id.startsWith("app:"));
        capturing = true;
        captureNext();
    }
    function captureNext() {
        if (!active || !pendingCaptures.length) { pendingCaptures = []; capturing = false; return; }
        const id = pendingCaptures[0];
        pendingCaptures = pendingCaptures.slice(1);
        const item = sources[id];
        if (!item || !item.Window.window?.visible || item.width <= 0 || item.height <= 0) {
            Qt.callLater(root.captureNext); return;
        }
        const width = item.width, height = item.height;
        // A frame at a time avoids overlapping render jobs on the same native window.
        const started = item.grabToImage(result => {
            if (root.active && root.sources[id] === item)
                root.previews = Object.assign({}, root.previews, {[id]: {result, url: result.url, width, height}});
            Qt.callLater(root.captureNext);
        });
        if (!started) Qt.callLater(root.captureNext);
    }
    Timer { interval: 750; repeat: true; running: root.active; triggeredOnStart: false; onTriggered: root.capture() }
}
