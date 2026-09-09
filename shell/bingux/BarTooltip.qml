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
    property point anchorPosition: Qt.point(0, 0)
    readonly property point followedPosition: {
        if (!shown) return Qt.point(0, 0);
        // A hovered widget can move while its container leaves Customise UI.
        // Follow its geometry without a polling timer or a second hover event.
        const geometry = [];
        for (let item = anchorItem; item; item = item.parent)
            geometry.push(item.x, item.y, item.width, item.height);
        if (barWindow) geometry.push(barWindow.width, barWindow.height, barWindow.margins.left, barWindow.margins.right, barWindow.margins.top, barWindow.margins.bottom);
        return barWindow ? DesktopEditing.point(anchorItem, barWindow, anchorItem.width / 2, anchorItem.height)
            : anchorItem.mapToGlobal(anchorItem.width / 2, anchorItem.height);
    }
    onFollowedPositionChanged: if (shown) anchorPosition = followedPosition
    readonly property real centreX: anchorPosition.x
    readonly property real bottomY: anchorPosition.y
    readonly property alias nativeWindow: popup
    property bool shown: false
    onRequestedChanged: {
        delay.stop();
        if (requested && Theme.tooltipDelay === 0) showTooltip();
        else if (requested) delay.start();
        else shown = false;
    }
    function showTooltip() {
        root.detectedReorderable = root.detectReorderable();
        root.shown = root.requested && root.text !== "";
    }
    Timer {
        id: delay
        interval: Theme.tooltipDelay
        onTriggered: root.showTooltip()
    }
    PanelWindow {
        id: popup
        visible: bubble.presented
        screen: root.barWindow?.screen || Quickshell.screens[0]
        implicitWidth: bubble.implicitWidth
        implicitHeight: bubble.implicitHeight
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        anchors { top: true; left: true }
        margins.top: root.barWindow?.anchors.bottom && !root.barWindow?.anchors.top
            ? Math.max(0, (root.barWindow.popupAnchorTop ?? root.bottomY - root.anchorItem.height) - height - Theme.gap)
            : root.bottomY + Theme.gap
        margins.left: Math.max(Theme.gap, Math.min(root.centreX - width / 2, (screen ? screen.width : 1920) - width - Theme.gap))
        mask: Region {}
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-bar-tooltip"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        TooltipBubble {
            id: bubble
            animated: true
            shown: root.shown
            transformOrigin: Item.Top
            anchors.fill: parent
            text: root.text
            supportingText: root.reorderable || root.detectedReorderable ? "Shift + right-click to customise" : ""
            wrapText: true
        }
    }
}
