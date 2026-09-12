pragma Singleton
import QtQuick
import QtQuick.Window
import Quickshell

Singleton {
    id: root
    property var editor: null
    readonly property bool active: !!editor && editor.visible
    readonly property var desktop: active ? Object.assign({}, editor.desktop, {
        layout: editor.layout
    }) : BinguxPreferences.data.desktop
    property var surfaces: []
    property var sources: ({})
    function registerSource(id, item) {
        sources = Object.assign({}, sources, {
            [id]: item
        });
    }
    function unregisterSource(id, item) {
        if (sources[id] !== item)
            return;
        const next = Object.assign({}, sources);
        delete next[id];
        sources = next;
    }
    function registerSurface(surface) {
        surfaces = surfaces.concat([surface]);
    }
    function unregisterSurface(surface) {
        surfaces = surfaces.filter(value => value !== surface);
    }
    function observeGeometry(item) {
        // mapToItem does not observe geometry changes. Containers with custom
        // transforms expose geometryRevision for their animation timeline.
        for (let current = item; current; current = current.parent) {
            // Read the properties to establish QML dependencies without an
            // array allocation for each ancestor on each animation frame.
            current.x;
            current.y;
            current.width;
            current.height;
            current.scale;
            current.rotation;
            current.transformOrigin;
            if ("geometryRevision" in current)
                current.geometryRevision;
        }
    }
    function point(item, window, x, y) {
        observeGeometry(item);
        const local = item.mapToItem(window.contentItem, x, y);
        return Qt.point(local.x + (window.anchors.right && !window.anchors.left ? window.screen.width - window.width - window.margins.right : window.margins.left), local.y + (window.anchors.bottom && !window.anchors.top ? window.screen.height - window.height - window.margins.bottom : window.margins.top));
    }
}
