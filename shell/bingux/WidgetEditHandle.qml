import QtQuick

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
    // The normal action stays disabled. Observe editing gestures from its
    // nearest enabled ancestor without adding an item to the container layout.
    TapHandler {
        id: disabledEdit
        parent: {
            let host = root.control;
            for (let item = root.control; item; item = item.parent)
                if (!item.enabled) host = item.parent;
            return host;
        }
        enabled: parent !== root.control && root.visible && root.control.visible
        acceptedButtons: Qt.RightButton
        acceptedModifiers: Qt.ShiftModifier
        onTapped: {
            if (root.control.contains(root.control.mapFromItem(parent, disabledEdit.point.position)))
                root.requested(root.widgetId, root.control);
        }
    }
}
