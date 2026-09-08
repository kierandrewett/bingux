import QtQuick

// Editing handles sit over the existing surface. Its controls stay in their own window.
MouseArea {
    id: root
    required property string zoneName
    required property var window
    property var entries: []
    property bool vertical: false
    property point start
    property string pressedId: ""
    objectName: "customise-zone-" + zoneName
    z: 10000
    visible: DesktopEditing.active
    preventStealing: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    cursorShape: pressedId ? Qt.ClosedHandCursor : Qt.ArrowCursor
    readonly property rect screenRect: {
        const dependencies = [x, y, width, height, window.width, window.height, window.margins.left, window.margins.right, window.margins.top, window.margins.bottom];
        const p = DesktopEditing.point(root, window, 0, 0);
        return Qt.rect(p.x, p.y, width, height);
    }
    function entryAt(x, y) {
        return entries.find(entry => {
            if (!entry.item || !entry.item.visible) return false;
            const p = root.mapToItem(entry.item, x, y);
            return p.x >= 0 && p.y >= 0 && p.x <= entry.item.width && p.y <= entry.item.height;
        });
    }
    function insertionIndex(point, id) {
        const items = entries.filter(entry => entry.id !== id && entry.item?.visible &&
            (zoneName !== "dock" || entry.id.startsWith("app:") === id.startsWith("app:"))).map(entry => ({entry, p: DesktopEditing.point(entry.item, window, 0, 0)}))
            .sort((a, b) => vertical ? a.p.y - b.p.y || a.p.x - b.p.x : a.p.x - b.p.x);
        const before = items.findIndex(value => {
            const p = value.p, item = value.entry.item;
            return vertical ? point.y < p.y || (point.y < p.y + item.height && point.x < p.x + item.width / 2) : point.x < p.x + item.width / 2;
        });
        return before < 0 ? items.length : before;
    }
    onPressed: mouse => {
        start = Qt.point(mouse.x, mouse.y);
        pressedId = entryAt(mouse.x, mouse.y)?.id || "";
    }
    onPositionChanged: mouse => {
        if (pressed && pressedId && (DesktopEditing.editor.draggedId || Math.abs(mouse.x - start.x) + Math.abs(mouse.y - start.y) > 6))
            DesktopEditing.editor.dragGlobal(pressedId, DesktopEditing.point(root, window, mouse.x, mouse.y));
    }
    onReleased: mouse => {
        const editor = DesktopEditing.editor;
        if (editor.draggedId) editor.release();
        else if (mouse.button === Qt.RightButton && pressedId) {
            editor.selectedWidget = pressedId; editor.optionsPage = "Widget";
        } else editor.selectContainer(zoneName);
        pressedId = "";
    }
    onCanceled: { if (DesktopEditing.editor) { DesktopEditing.editor.draggedId = ""; DesktopEditing.editor.hoverZone = ""; } pressedId = ""; }
    Rectangle {
        anchors.fill: parent; radius: typeof root.parent.radius === "number" ? root.parent.radius : 0; color: "transparent"
        border.width: 1
        border.color: DesktopEditing.editor?.hoverZone === root.zoneName ? Theme.accent : Theme.outline
    }
    Component.onCompleted: DesktopEditing.registerSurface(root)
    Component.onDestruction: DesktopEditing.unregisterSurface(root)
}
