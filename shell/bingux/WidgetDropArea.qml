import QtQuick

DropArea {
    id: root
    required property string zoneName
    required property var window
    property var surface: null
    enabled: DesktopEditing.active
    keys: ["application/x-bingux-widget"]
    function updatePosition(event) {
        const editor = DesktopEditing.editor;
        if (DesktopEditing.active && editor.draggedId) editor.pointer = DesktopEditing.point(root, window, event.x, event.y);
        if (!DesktopEditing.active || !editor.draggedId || !editor.accepts(editor.draggedId, zoneName)) {
            event.accepted = false;
            return false;
        }
        const point = DesktopEditing.point(root, window, event.x, event.y);
        if (surface) {
            const rect = surface.screenRect;
            if (point.x < rect.x || point.y < rect.y || point.x > rect.x + rect.width || point.y > rect.y + rect.height) {
                event.accepted = false;
                if (editor.hoverZone === zoneName) editor.hoverZone = "";
                return false;
            }
        }
        editor.pointer = point;
        editor.hoverZone = zoneName;
        editor.hoverIndex = surface ? surface.insertionIndex(point, editor.draggedId) : 0;
        event.accepted = true;
        return true;
    }
    onEntered: drag => updatePosition(drag)
    onPositionChanged: drag => updatePosition(drag)
    onExited: if (DesktopEditing.editor?.hoverZone === zoneName) DesktopEditing.editor.hoverZone = ""
    onDropped: drop => {
        if (!updatePosition(drop)) return;
        DesktopEditing.editor.release();
        drop.accept(Qt.MoveAction);
    }
}
