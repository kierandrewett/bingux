import QtQuick
import QtQuick.Window

MouseArea {
    id: root
    required property Item control
    required property string widgetId
    property bool previewSource: true
    parent: control
    anchors.fill: parent
    z: 1000
    acceptedButtons: Qt.RightButton
    Component.onCompleted: if (previewSource) DesktopEditing.registerSource(widgetId, control)
    Component.onDestruction: if (previewSource) DesktopEditing.unregisterSource(widgetId, control)
    signal requested(string widgetId, var control)
    onPressed: mouse => { if (!(mouse.modifiers & Qt.ShiftModifier)) mouse.accepted = false; }
    onClicked: requested(widgetId, control)
    // Observe only the edit gesture at window level. Check state on the event
    // so this handler does not join the control's layout and enabled bindings.
    TapHandler {
        id: disabledEdit
        // A null parent destroys a handler created before its window exists.
        parent: root.control.Window.window?.contentItem || root
        acceptedButtons: Qt.RightButton
        acceptedModifiers: Qt.ShiftModifier
        onTapped: {
            if (!root.visible) return;
            let disabled = false;
            for (let item = root.control; item; item = item.parent) {
                if (!item.visible) return;
                if (!item.enabled) disabled = true;
            }
            if (!disabled) return;
            const point = disabledEdit.point.position;
            if (!root.control.contains(root.control.mapFromItem(parent, point))) return;
            for (let item = root.control.parent; item; item = item.parent)
                if (item.clip && !item.contains(item.mapFromItem(parent, point))) return;
            root.requested(root.widgetId, root.control);
        }
    }
}
