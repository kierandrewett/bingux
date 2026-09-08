import QtQuick

MouseArea {
    required property Item control
    required property string widgetId
    parent: control
    anchors.fill: parent
    z: 1000
    acceptedButtons: Qt.RightButton
    Component.onCompleted: DesktopEditing.registerSource(widgetId, control)
    Component.onDestruction: DesktopEditing.unregisterSource(widgetId, control)
    signal requested(string widgetId, var control)
    onPressed: mouse => { if (!(mouse.modifiers & Qt.ShiftModifier)) mouse.accepted = false; }
    onClicked: requested(widgetId, control)
}
