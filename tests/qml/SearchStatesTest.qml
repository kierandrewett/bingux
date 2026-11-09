import QtQuick
import Quickshell
import Quickshell.Wayland

// Renders SearchOverlay in one body state, chosen by the SEARCH_STATE env var:
// offline (service not connected), empty (finished query, no results),
// error (query failed with nothing to list), searching (query in flight).
// Each state sets the overlay's own properties, the same way the navigation
// fixtures do, so the real bodyState binding picks the message.
SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-search-states-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    readonly property string scene: Quickshell.env("SEARCH_STATE") || "offline"
    function find(item, name) {
        if (item.objectName === name)
            return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found)
                return found;
        }
        return null;
    }
    // The socket is an id inside SearchOverlay.qml, so find it among children.
    function socket() {
        for (let index = 0; index < preview.data.length; index++) {
            const child = preview.data[index];
            if (child.connectionState !== undefined)
                return child;
        }
        return null;
    }
    Timer {
        interval: 1500
        running: true
        onTriggered: {
            const input = preview.find(preview.contentItem, "searchInput");
            input.text = preview.scene === "empty" ? "zzqx" : "files";
            input.cursorPosition = input.length;
            if (preview.scene !== "offline")
                preview.socket().connectionState = "ready";
            if (preview.scene === "empty") {
                preview.queryError = "";
                preview.queryComplete = true;
                preview.results = [];
                preview.displayedResults = [];
            } else if (preview.scene === "error") {
                preview.queryComplete = true;
                preview.queryError = "Search unavailable.";
                preview.results = [];
                preview.displayedResults = [];
            } else if (preview.scene === "searching") {
                preview.queryError = "";
                preview.activeRequestId = "scene-request";
                preview.queryComplete = false;
                preview.results = [];
                preview.displayedResults = [];
            }
        }
    }
    Component.onCompleted: console.warn("SEARCH_STATES_READY " + scene)
}
