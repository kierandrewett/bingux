import QtQuick
import Quickshell

// Keep the exact grabbed frame alive until the compositor ends the native drag.
Item {
    id: root
    property string widgetId: ""
    property bool running: false
    property bool preparing: false
    property int generation: 0
    property var grabbedFrame: null
    function begin(id, source, window, position) {
        if (running || preparing || !DesktopEditing.active || !source)
            return;
        widgetId = id;
        preparing = true;
        const attempt = ++generation;
        const origin = DesktopEditing.point(source, window, 0, 0);
        const size = Qt.size(Math.max(1, Math.ceil(source.width)), Math.max(1, Math.ceil(source.height)));
        const started = source.grabToImage(frame => {
            if (attempt !== generation || !preparing || !DesktopEditing.active)
                return;
            preparing = false;
            grabbedFrame = frame;
            const editor = DesktopEditing.editor;
            editor.dragImage = frame.url;
            editor.dragSize = size;
            const offset = Qt.point(position.x - origin.x, position.y - origin.y);
            editor.dragHotSpot = offset.x >= 0 && offset.y >= 0 && offset.x <= size.width && offset.y <= size.height ? offset : Qt.point(size.width / 2, size.height / 2);
            editor.dragGlobal(id, position);
            running = true;
            Drag.active = true;
        }, size);
        if (!started)
            preparing = false;
    }
    function cancelPending() {
        preparing = false;
        generation++;
    }
    Drag.dragType: Drag.Automatic
    Drag.mimeData: ({
            "application/x-bingux-widget": widgetId
        })
    Drag.supportedActions: Qt.MoveAction
    Drag.proposedAction: Qt.MoveAction
    Drag.onDragFinished: {
        running = false;
        grabbedFrame = null;
        if (DesktopEditing.editor) {
            DesktopEditing.editor.dragImage = "";
            DesktopEditing.editor.draggedId = "";
            DesktopEditing.editor.hoverZone = "";
        }
    }
}
