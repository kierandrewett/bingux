pragma Singleton
import QtQuick
import Quickshell

QtObject {
    id: root
    property var registrations: []
    function add(item) { registrations = registrations.concat([item]); Qt.callLater(flush); }
    function remove(item) {
        if (item.published && session.capabilities.includes("blur-regions"))
            session.send(Object.assign({}, item.published, {region: null}));
        registrations = registrations.filter(entry => entry !== item);
    }
    function reset() {
        for (const item of registrations) item.published = null;
        Qt.callLater(flush);
    }
    function flush() {
        if (!session.ready || !session.capabilities.includes("blur-regions")) return;
        for (const item of registrations) {
            const screen = item.window?.screen;
            const record = screen ? {op: "blur-region", namespace: item.surfaceNamespace,
                screen: [screen.x, screen.y],
                region: item.window.visible ? [item.region.x, item.region.y, item.region.width, item.region.height] : null} : null;
            const old = item.published;
            if (old && (!record || old.namespace !== record.namespace || JSON.stringify(old.screen) !== JSON.stringify(record.screen)))
                session.send(Object.assign({}, old, {region: null}));
            if (record && JSON.stringify(old) !== JSON.stringify(record)) session.send(record);
            item.published = record;
        }
    }
    property var session: ShortcutSession {
        onReadyChanged: if (ready) Qt.callLater(root.flush)
        onCapabilitiesChanged: root.reset()
    }
}
