import QtQuick
import QtQuick.Controls
import "DesktopLayout.js" as DesktopLayout

// Editing handles sit over the existing surface. Its controls stay in their own window.
MouseArea {
    id: root
    required property string zoneName
    required property var window
    property var entries: []
    property bool vertical: false
    property Item geometryItem: root
    property point start
    property string pressedId: ""
    property bool hadDrag: false
    objectName: "customise-zone-" + zoneName
    z: 10000
    visible: DesktopEditing.active
    preventStealing: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    ContextMenu.menu: null
    ContextMenu.onRequested: position => {
        const id = entryAt(position.x, position.y)?.id;
        if (id && DesktopEditing.active) {
            DesktopEditing.editor.selectedWidget = id;
            DesktopEditing.editor.optionsPage = "Widget";
        }
    }
    cursorShape: pressedId ? Qt.ClosedHandCursor : Qt.ArrowCursor
    readonly property rect screenRect: {
        const dependencies = [geometryItem.x, geometryItem.y, geometryItem.width, geometryItem.height, window.width, window.height, window.margins.left, window.margins.right, window.margins.top, window.margins.bottom];
        const p = DesktopEditing.point(geometryItem, window, 0, 0);
        return Qt.rect(p.x, p.y, geometryItem.width, geometryItem.height);
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
        const editor = DesktopEditing.editor;
        const order = zoneName === "control-centre" ? (editor.desktop.controlOrder || DesktopLayout.controlOrder()).map(name => "control-" + name)
            : zoneName === "dock" && id.startsWith("app:") ? editor.dockApplications : editor.layout[zoneName] || [];
        return DesktopLayout.insertionIndex(order, id, items.map(value => value.entry.id), before);
    }
    onPressed: mouse => {
        hadDrag = false;
        start = Qt.point(mouse.x, mouse.y);
        pressedId = entryAt(mouse.x, mouse.y)?.id || "";
    }
    onPositionChanged: mouse => {
        if ((pressedButtons & Qt.LeftButton) && pressedId && !hadDrag && Math.abs(mouse.x - start.x) + Math.abs(mouse.y - start.y) > 6) {
            hadDrag = true;
            nativeDrag.begin(pressedId, entryAt(start.x, start.y)?.item, window, DesktopEditing.point(root, window, start.x, start.y));
        }
    }
    onReleased: mouse => {
        const editor = DesktopEditing.editor;
        if (hadDrag) { nativeDrag.cancelPending(); pressedId = ""; return; }
        if (mouse.button === Qt.RightButton && pressedId) {
            editor.selectedWidget = pressedId; editor.optionsPage = "Widget";
        } else editor.selectContainer(zoneName);
        pressedId = "";
    }
    onCanceled: { nativeDrag.cancelPending(); pressedId = ""; }
    WidgetDrag { id: nativeDrag }
    WidgetDropArea { anchors.fill: parent; zoneName: root.zoneName; window: root.window; surface: root }
    readonly property bool dropActive: DesktopEditing.editor?.hoverZone === zoneName
    readonly property rect insertionRect: {
        if (!dropActive) return Qt.rect(0, 0, 0, 0);
        const editor = DesktopEditing.editor;
        const order = zoneName === "control-centre" ? (editor.desktop.controlOrder || DesktopLayout.controlOrder()).map(name => "control-" + name)
            : zoneName === "dock" && editor.draggedId.startsWith("app:") ? editor.dockApplications : editor.layout[zoneName] || [];
        const remaining = order.filter(id => id !== editor.draggedId);
        const next = remaining.slice(editor.hoverIndex).map(id => entries.find(entry => entry.id === id)).find(entry => entry?.item?.visible);
        const previous = remaining.slice(0, editor.hoverIndex).reverse().map(id => entries.find(entry => entry.id === id)).find(entry => entry?.item?.visible);
        const item = next?.item || previous?.item;
        if (!item) return vertical ? Qt.rect(6, 6, width - 12, 3) : Qt.rect(6, 5, 3, height - 10);
        const p = item.mapToItem(root, 0, 0);
        return vertical ? Qt.rect(Math.max(4, p.x), p.y + (next ? 0 : item.height) - 2, Math.min(width - 8, item.width), 3)
            : Qt.rect(p.x + (next ? 0 : item.width) - 2, 5, 3, height - 10);
    }
    Rectangle {
        visible: root.dropActive
        x: Math.max(1, Math.min(root.insertionRect.x, root.width - width - 1))
        y: Math.max(1, Math.min(root.insertionRect.y, root.height - height - 1))
        width: Math.max(0, root.insertionRect.width); height: Math.max(0, root.insertionRect.height)
        radius: 1.5; color: Theme.accent
    }
    Rectangle {
        anchors.fill: parent; radius: typeof root.parent.radius === "number" ? root.parent.radius : 0; color: "transparent"
        border.width: 1
        border.color: root.dropActive || (DesktopEditing.editor?.optionsPage === "Container" && DesktopEditing.editor.selectedContainer === root.zoneName) ? Theme.accent : Theme.outline
    }
    Component.onCompleted: DesktopEditing.registerSurface(root)
    Component.onDestruction: DesktopEditing.unregisterSurface(root)
}
