import QtQuick

// Ctrl reserves the gesture for rearranging; ordinary presses reach the control.
MouseArea {
    id: root
    required property var controller
    required property Item control
    objectName: "barReorderHandle"
    parent: control
    anchors.fill: parent
    z: 100
    acceptedButtons: Qt.LeftButton
    preventStealing: pressed
    property point startPoint
    property bool dragging: false
    property bool cancelled: false
    property point dropPoint
    cursorShape: dragging ? Qt.ClosedHandCursor : Qt.ArrowCursor
    onPressed: mouse => {
        if (!(mouse.modifiers & Qt.ControlModifier)) {
            mouse.accepted = false;
            return;
        }
        if (controller.draggedControl) {
            mouse.accepted = true;
            cancelled = true;
            return;
        }
        startPoint = mapToGlobal(mouse.x, mouse.y);
        dropPoint = startPoint;
        dragging = false;
        cancelled = false;
    }
    onPositionChanged: mouse => {
        if (!pressed || cancelled)
            return;
        dropPoint = mapToGlobal(mouse.x, mouse.y);
        if (!dragging && Math.hypot(dropPoint.x - startPoint.x, dropPoint.y - startPoint.y) >= 8)
            dragging = controller.beginReorder(control);
        if (dragging)
            controller.updateReorder(control, dropPoint, startPoint);
    }
    onReleased: {
        if (dragging && !cancelled)
            controller.finishReorder(control, dropPoint, false);
        dragging = false;
    }
    onCanceled: {
        if (dragging)
            controller.finishReorder(control, dropPoint, true);
        cancelled = true;
        dragging = false;
    }
    // The actual control moves; no duplicate image or insertion-marker proxy.
    Component.onCompleted: {
        control.transform = [dragTranslation, neighbourTranslation];
    }
    Binding {
        target: root.control
        property: "z"
        value: root.controller.draggedControl === root.control ? 10 : 0
    }
    Translate {
        id: dragTranslation
        readonly property bool moving: root.controller.draggedControl === root.control
        x: moving && !root.controller.overflows(root.control) ? root.controller.dragOffset : 0
        y: moving && root.controller.overflows(root.control) ? root.controller.dragOffset : 0
    }
    ReorderSlide {
        id: neighbourTranslation
        animate: !root.controller.committingReorder
        readonly property real offset: root.controller.draggedControl === root.control ? 0 : root.controller.reorderShift(root.control)
        x: root.controller.overflows(root.control) ? 0 : offset
        y: root.controller.overflows(root.control) ? offset : 0
    }
}
