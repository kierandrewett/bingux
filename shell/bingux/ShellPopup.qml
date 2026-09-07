import QtQuick
import Quickshell
import Quickshell.Wayland

// A layer surface avoids native xdg-popup grabs on layer-shell parents.
PanelWindow {
    id: root
    default property alias contents: body.data
    property real preferredX: (width - popupWidth) / 2
    property real preferredY: Theme.barHeight + Theme.gap
    property real cornerRadius: Theme.cardRadius
    property int contentPadding: Theme.padding
    readonly property real contentRadius: Theme.insetRadius(cornerRadius, contentPadding)
    property int popupWidth: 320
    property int popupHeight: body.childrenRect.height + contentPadding * 2
    readonly property alias body: body
    visible: false
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "gnoblin-shell-popup"
    // Keep menus above the desktop without taking the active application's
    // keyboard focus when they open. Gnoblin's OnDemand semantics focus this
    // surface after the user clicks inside it, preserving Escape and keyboard
    // navigation while the menu is being used.
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    anchors { top: true; bottom: true; left: true; right: true }
    onVisibleChanged: {
        if (visible) {
            contentItem.forceActiveFocus();
            reveal.restart();
        } else reveal.stop();
    }
    ParallelAnimation {
        id: reveal
        NumberAnimation { target: card; property: "scale"; from: 0.9; to: 1; duration: 160; easing.type: Easing.OutCubic }
        NumberAnimation { target: card; property: "opacity"; from: 0; to: 1; duration: 160; easing.type: Easing.OutCubic }
    }
    contentItem.Keys.onEscapePressed: visible = false
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons; onClicked: root.visible = false }
    Rectangle {
        id: card
        x: Math.max(Theme.gap, Math.min(root.preferredX, root.width - width - Theme.gap))
        y: Math.max(Theme.gap, Math.min(root.preferredY, root.height - height - Theme.gap))
        width: Math.min(root.popupWidth, root.width - Theme.gap * 2)
        height: Math.min(root.popupHeight, root.height - Theme.gap * 2)
        radius: root.cornerRadius
        color: Theme.surface
        border.color: Theme.outline
        border.width: 1
        MouseArea { anchors.fill: parent }
        Item {
            id: body
            anchors.fill: parent
            anchors.margins: root.contentPadding
        }
    }
}
