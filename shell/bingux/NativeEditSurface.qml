import QtQuick
import QtQuick.Controls
import "DesktopLayout.js" as DesktopLayout

// Editing handles sit over the existing surface. Its controls stay in their own window.
MouseArea {
    id: root
    required property string zoneName
    required property var window
    property var entries: []
    // Overflow exposes existing placements without becoming a saved container.
    property bool sourceOnly: false
    readonly property var expandedEntries: {
        const result = [];
        for (const entry of entries)
            for (const member of (entry.item?.memberEntries || []).concat([entry]))
                if (!result.some(other => other.id === member.id)) result.push(member);
        return result;
    }
    property bool vertical: false
    property Item geometryItem: root
    property point start
    property string pressedId: ""
    property bool hadDrag: false
    objectName: "customise-zone-" + zoneName
    z: 10000
    visible: DesktopEditing.active
    onVisibleChanged: if (!visible) appActions.visible = false
    preventStealing: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    ContextMenu.menu: null
    ContextMenu.onRequested: position => {
        const id = entryAt(position.x, position.y)?.id;
        if (id && DesktopEditing.active) {
            inspect(id);
        }
    }
    function inspect(id) {
        if (id.startsWith("app:")) {
            appActions.widgetId = id;
            appActions.anchorItem = entries.find(entry => entry.id === id)?.item || root;
            appActions.visible = true;
        } else {
            DesktopEditing.editor.selectedWidget = id;
            DesktopEditing.editor.optionsPage = "Widget";
        }
    }
    TrayMenu {
        id: appActions
        objectName: "customiseAppActions"
        property string widgetId: ""
        screen: root.window.screen
        anchorWindow: root.window
        actions: [
            {text: "Unpin from dock", enabled: !!DesktopEditing.editor?.dockApplications.includes(widgetId),
                triggered: () => DesktopEditing.editor.put(widgetId, "palette", 0)},
            {text: "Customise…", icon: "preferences-system-symbolic", enabled: true,
                triggered: () => { DesktopEditing.editor.selectedWidget = widgetId; DesktopEditing.editor.optionsPage = "Widget"; }}
        ].map(action => Object.assign({isSeparator: false, hasChildren: false, checkState: Qt.Unchecked}, action))
    }
    cursorShape: pressedId ? Qt.ClosedHandCursor : Qt.ArrowCursor
    readonly property rect screenRect: {
        // Detached containers return to their layer window when editing starts.
        // Their inactive floating host has no desktop-space layer margins.
        if (!window || !("anchors" in window)) return Qt.rect(0, 0, 0, 0);
        const dependencies = [geometryItem.x, geometryItem.y, geometryItem.width, geometryItem.height, window.width, window.height, window.margins.left, window.margins.right, window.margins.top, window.margins.bottom];
        const p = DesktopEditing.point(geometryItem, window, 0, 0);
        const end = DesktopEditing.point(geometryItem, window, geometryItem.width, geometryItem.height);
        return Qt.rect(p.x, p.y, end.x - p.x, end.y - p.y);
    }
    readonly property var groups: entries.filter(entry => entry.item?.memberEntries && entry.item.visible)
    function groupVisible(entry) {
        DesktopEditing.observeGeometry(entry.item);
        const point = entry.item.mapToItem(root, 0, 0);
        return point.x + entry.item.width > 0 && point.x < width && point.y + entry.item.height > 0 && point.y < height;
    }
    function groupHandleRect(entry) {
        DesktopEditing.observeGeometry(entry.item);
        const point = entry.item.mapToItem(root, 0, 0);
        return Qt.rect(Math.max(0, point.x + entry.item.width / 2 - 14), Math.max(0, point.y - 4), 28, 8);
    }
    function entryAt(x, y) {
        const group = groups.find(entry => {
            if (!groupVisible(entry)) return false;
            const rect = groupHandleRect(entry);
            return x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height;
        });
        if (group) return group;
        return expandedEntries.find(entry => {
            if (!entry.item || !entry.item.visible) return false;
            const p = root.mapToItem(entry.item, x, y);
            const paddingX = Math.max(0, (12 - entry.item.width) / 2);
            const paddingY = Math.max(0, (12 - entry.item.height) / 2);
            return p.x >= -paddingX && p.y >= -paddingY && p.x <= entry.item.width + paddingX && p.y <= entry.item.height + paddingY;
        });
    }
    function destinationAt(point, id) {
        const group = entries.find(entry => {
            if (!entry.item?.memberEntries || !entry.item.visible || !DesktopEditing.editor.accepts(id, entry.id)) return false;
            const p = DesktopEditing.point(entry.item, window, 0, 0);
            return point.x >= p.x && point.y >= p.y && point.x <= p.x + entry.item.width && point.y <= p.y + entry.item.height;
        });
        return group?.id || zoneName;
    }
    function insertionIndex(point, id, target = zoneName) {
        const order = DesktopEditing.editor.orderFor(target, id);
        const items = expandedEntries.filter(entry => order.includes(entry.id) && entry.id !== id && entry.item?.visible &&
            (zoneName !== "dock" || entry.id.startsWith("app:") === id.startsWith("app:"))).map(entry => ({entry, p: DesktopEditing.point(entry.item, window, 0, 0)}))
            .sort((a, b) => vertical ? a.p.y - b.p.y || a.p.x - b.p.x : a.p.x - b.p.x);
        const before = items.findIndex(value => {
            const p = value.p, item = value.entry.item;
            return vertical ? point.y < p.y || (point.y < p.y + item.height && point.x < p.x + item.width / 2) : point.x < p.x + item.width / 2;
        });
        return DesktopLayout.insertionIndex(order, id, items.map(value => value.entry.id), before);
    }
    Repeater {
        model: root.groups
        Item {
            id: grip
            required property var modelData
            objectName: "customise-group-handle-" + modelData.id
            readonly property rect bounds: root.groupHandleRect(modelData)
            visible: root.groupVisible(modelData)
            x: bounds.x; y: bounds.y; width: bounds.width; height: bounds.height
            Rectangle { anchors.centerIn: parent; width: 24; height: 4; radius: 2; color: gripHover.hovered ? Theme.accent : Theme.muted }
            HoverHandler { id: gripHover; cursorShape: Qt.OpenHandCursor }
            ShellTooltip { parent: grip; visible: DesktopEditing.active && gripHover.hovered; text: "Drag " + (DesktopLayout.widget(grip.modelData.id)?.label || "group") }
        }
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
            inspect(pressedId);
        } else {
            const target = expandedEntries.find(entry => entry.id === pressedId)?.item;
            if (typeof target?.activateInEditor === "function") target.activateInEditor();
            else if (sourceOnly) editor.selectContainer(editor.containerFor(pressedId || zoneName));
            else editor.selectContainer(entries.find(entry => entry.item?.memberEntries && (entry.id === pressedId || entry.item.memberEntries.some(member => member.id === pressedId)))?.id || zoneName);
        }
        pressedId = "";
    }
    onCanceled: { nativeDrag.cancelPending(); pressedId = ""; }
    WidgetDrag { id: nativeDrag }
    WidgetDropArea { enabled: DesktopEditing.active && !root.sourceOnly; anchors.fill: parent; zoneName: root.zoneName; window: root.window; surface: root }
    readonly property bool dropActive: DesktopEditing.editor?.hoverZone === zoneName || entries.some(entry => entry.item?.memberEntries && entry.id === DesktopEditing.editor?.hoverZone)
    readonly property rect insertionRect: {
        if (!dropActive) return Qt.rect(0, 0, 0, 0);
        const editor = DesktopEditing.editor;
        const order = editor.orderFor(editor.hoverZone, editor.draggedId);
        const remaining = order.filter(id => id !== editor.draggedId);
        const next = remaining.slice(editor.hoverIndex).map(id => expandedEntries.find(entry => entry.id === id)).find(entry => entry?.item?.visible);
        const previous = remaining.slice(0, editor.hoverIndex).reverse().map(id => expandedEntries.find(entry => entry.id === id)).find(entry => entry?.item?.visible);
        const item = next?.item || previous?.item;
        if (!item) return vertical ? Qt.rect(6, 6, width - 12, 3) : Qt.rect(6, 5, 3, height - 10);
        DesktopEditing.observeGeometry(item);
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
        objectName: "customiseContainerOutline"
        readonly property Item target: root.entries.find(entry => entry.item?.memberEntries && entry.id === (root.dropActive ? DesktopEditing.editor.hoverZone : DesktopEditing.editor?.selectedContainer))?.item || root.geometryItem
        readonly property point origin: {
            DesktopEditing.observeGeometry(target);
            DesktopEditing.observeGeometry(root);
            return target.mapToItem(root, 0, 0);
        }
        x: origin.x; y: origin.y; width: target.width; height: target.height
        radius: typeof root.geometryItem.radius === "number" ? root.geometryItem.radius : typeof root.parent.radius === "number" ? root.parent.radius : 0; color: "transparent"
        border.width: 1
        border.color: root.dropActive || (DesktopEditing.editor?.optionsPage === "Container" && (DesktopEditing.editor.selectedContainer === root.zoneName || target !== root.geometryItem)) ? Theme.accent : Theme.outline
    }
    Component.onCompleted: DesktopEditing.registerSurface(root)
    Component.onDestruction: DesktopEditing.unregisterSurface(root)
}
