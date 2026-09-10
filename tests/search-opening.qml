import QtQuick
import Quickshell
import "../shell/bingux"

ShellRoot {
    SearchOverlay { id: search }
    property var positions: []
    property int openings: 0
    property var surface: null
    function find(item) {
        if (item.objectName === "searchSurface") return item;
        for (const child of item.children || []) { const result = find(child); if (result) return result; }
        return null;
    }
    Timer {
        interval: 250; running: true
        onTriggered: { surface = find(search.contentItem); search.showSearch(); }
    }
    FrameAnimation {
        running: search.visible && !search.closing && surface !== null && openings < 3
        onTriggered: {
            if (search.width <= 0 || search.height <= 0) return;
            positions.push(surface.y);
            if (positions.length < 12) return;
            if (Math.max(...positions) - Math.min(...positions) > 0.5) {
                console.error("Search moved during opening:", JSON.stringify(positions));
                Qt.exit(1);
                return;
            }
            console.info("PASS: search appears at a fixed position", JSON.stringify(positions));
            search.closeSearch();
            if (++openings === 3) Qt.quit(); else reopen.restart();
        }
    }
    Timer { id: reopen; interval: 200; onTriggered: { positions = []; search.showSearch(); } }
    Timer { interval: 8000; running: true; onTriggered: Qt.exit(2) }
}
