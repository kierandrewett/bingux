import QtQuick
import Quickshell

// Native drag-and-drop keeps the gesture alive between separate layer windows.
Item {
    id: root
    property string widgetId: ""
    property bool running: false
    function begin(id, position) {
        if (running || !DesktopEditing.active) return;
        widgetId = id;
        DesktopEditing.editor.dragGlobal(id, position);
        running = true;
        Drag.imageSource = widgetId.startsWith("app:") ? Quickshell.iconPath(DesktopEditing.editor.baseInfo(widgetId)?.icon || "application-x-executable") : DesktopEditing.previews[widgetId]?.url || "";
        Drag.active = true;
    }
    Drag.dragType: Drag.Automatic
    Drag.mimeData: ({"application/x-bingux-widget": widgetId})
    Drag.supportedActions: Qt.MoveAction
    Drag.proposedAction: Qt.MoveAction
    Drag.onDragFinished: {
        running = false;
        if (DesktopEditing.editor) {
            DesktopEditing.editor.draggedId = "";
            DesktopEditing.editor.hoverZone = "";
        }
    }
}
