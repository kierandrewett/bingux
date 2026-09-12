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
        if (DesktopEditing.active && editor.draggedId)
            editor.pointer = DesktopEditing.point(root, window, event.x, event.y);
        const point = DesktopEditing.point(root, window, event.x, event.y);
        const destination = surface && DesktopEditing.active ? surface.destinationAt(point, editor.draggedId) : zoneName;
        if (!DesktopEditing.active || !editor.draggedId || !editor.accepts(editor.draggedId, destination)) {
            event.accepted = false;
            return false;
        }
        if (surface) {
            const rect = surface.screenRect;
            if (point.x < rect.x || point.y < rect.y || point.x > rect.x + rect.width || point.y > rect.y + rect.height) {
                event.accepted = false;
                if (editor.hoverZone === zoneName || surface.dropActive)
                    editor.hoverZone = "";
                return false;
            }
        }
        editor.pointer = point;
        editor.hoverZone = destination;
        editor.hoverIndex = surface ? surface.insertionIndex(point, editor.draggedId, destination) : 0;
        event.accepted = true;
        return true;
    }
    onEntered: drag => updatePosition(drag)
    onPositionChanged: drag => updatePosition(drag)
    onExited: if (DesktopEditing.editor && (DesktopEditing.editor.hoverZone === zoneName || surface?.dropActive))
        DesktopEditing.editor.hoverZone = ""
    onDropped: drop => {
        if (!updatePosition(drop))
            return;
        DesktopEditing.editor.release();
        drop.accept(Qt.MoveAction);
    }
}
