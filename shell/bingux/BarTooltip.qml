import QtQuick
import Quickshell
import Quickshell.Wayland

// A bar is only 32px tall: tooltips need their own input-transparent surface.
Scope {
    id: root
    required property Item anchorItem
    required property var barWindow
    property bool requested: false
    property string text: ""
    property bool reorderable: false
    property bool detectedReorderable: false
    function detectReorderable() {
        // Tray and privacy icons live inside a movable group. Inspect ancestors
        // when opening, after the shared drag handles have been created.
        for (let item = anchorItem; item; item = item.parent) {
            if (item.children.some(child => child.objectName === "barReorderHandle")) return true;
        }
        return false;
    }
    property real centreX: 0
    property real bottomY: 0
    Component.onDestruction: Theme.endTooltip(root)
    onRequestedChanged: {
        delay.stop();
        if (requested && Theme.tooltipDelay === 0) showTooltip();
        else if (requested) delay.start();
        else popup.visible = false;
    }
    function showTooltip() {
        root.detectedReorderable = root.detectReorderable();
        const point = root.anchorItem.mapToGlobal(root.anchorItem.width / 2, root.anchorItem.height);
        root.centreX = point.x;
        root.bottomY = point.y;
        popup.visible = root.requested && root.text !== "";
    }
    Timer {
        id: delay
        interval: Theme.tooltipDelay
        onTriggered: root.showTooltip()
    }
    PanelWindow {
        id: popup
        onVisibleChanged: {
            reveal.stop();
            if (visible) {
                reveal.duration = Theme.beginTooltip(root);
                bubble.opacity = reveal.duration > 0 ? 0 : 1;
                if (reveal.duration > 0) reveal.start();
            } else Theme.endTooltip(root);
        }
        visible: false
        screen: root.barWindow.screen
        implicitWidth: bubble.implicitWidth
        implicitHeight: bubble.implicitHeight
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        anchors { top: true; left: true }
        margins.top: root.bottomY + Theme.gap
        margins.left: Math.max(Theme.gap, Math.min(root.centreX - width / 2, (screen ? screen.width : 1920) - width - Theme.gap))
        mask: Region {}
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-bar-tooltip"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        TooltipBubble {
            id: bubble
            anchors.fill: parent
            text: root.text
            supportingText: root.reorderable || root.detectedReorderable ? "Ctrl + drag to move" : ""
            wrapText: true
        }
        NumberAnimation { id: reveal; target: bubble; property: "opacity"; to: 1; easing.type: Easing.OutCubic }
    }
}
