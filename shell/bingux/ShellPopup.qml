import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Wayland

// `visible` is the requested state. The layer window stays mapped only long
// enough to finish closing, with input released as soon as closing starts.
Scope {
    id: root
    default property alias contents: body.data
    property bool visible: false
    property bool keepWindowAlive: false
    property bool keyboardInteractive: true
    property bool dismissOnOutsideClick: true
    property alias screen: window.screen
    readonly property alias nativeWindow: window
    property Item hostItem: null
    property var anchorWindow: null
    property Item anchorItem: null
    property int anchorAlignment: Qt.AlignRight
    readonly property real placementLeft: anchorWindow ? anchorWindow.margins.left : 0
    readonly property real placementRight: width - (anchorWindow ? anchorWindow.margins.right : 0)
    readonly property real belowAnchorY: anchorItem ? anchorPosition.y + Theme.gap : Theme.barHeight + Theme.gap
    readonly property bool anchorAbove: !!anchorWindow && anchorWindow.anchors.bottom && !anchorWindow.anchors.top
    readonly property real anchorTop: anchorAbove && typeof anchorWindow.popupAnchorTop === "number" ? anchorWindow.popupAnchorTop : anchorPosition.y - (anchorItem ? anchorItem.height : 0)
    readonly property point anchorPosition: {
        // Reparenting a launcher must not anchor a popup to its own content.
        if (!anchorItem || anchorItem.Window.window === window.contentItem.Window.window) return Qt.point(0, Theme.barHeight);
        // Track layout and reparenting, including controls moved into overflow.
        for (let item = anchorItem; item; item = item.parent) {
            const geometry = [item.x, item.y, item.width, item.height];
        }
        const x = anchorAlignment === Qt.AlignHCenter ? anchorItem.width / 2 : anchorItem.width;
        if (anchorWindow && anchorItem.Window.window === anchorWindow.contentItem.Window.window) {
            // Convert bar-local coordinates with the layer-surface margins.
            const point = anchorItem.mapToItem(anchorWindow.contentItem, x, anchorItem.height);
            return Qt.point(placementLeft + point.x, (anchorWindow.anchors.bottom && !anchorWindow.anchors.top ? height - anchorWindow.height - anchorWindow.margins.bottom : anchorWindow.margins.top) + point.y);
        }
        return anchorItem.mapToItem(contentItem, x, anchorItem.height);
    }
    // Companion surfaces share one reveal timeline and one screen-space origin.
    property var motionSource: null
    Connections {
        target: root.motionSource
        function onRetainedChanged() { if (!root.motionSource.retained) root.retained = false; }
    }
    readonly property real width: hostItem ? hostItem.width : window.width
    readonly property real height: hostItem ? hostItem.height : window.height
    readonly property var contentItem: hostItem ? hostItem : window.contentItem
    readonly property alias body: body
    readonly property bool closing: retained && !visible
    property real preferredX: anchorItem ? anchorPosition.x - popupWidth / (anchorAlignment === Qt.AlignHCenter ? 2 : 1) : (width - popupWidth) / 2
    property real preferredY: anchorItem ? (anchorAbove ? anchorTop - popupHeight - Theme.gap : anchorPosition.y + Theme.gap) : Theme.barHeight + Theme.gap
    property bool surfaceVisible: true
    property color surfaceColor: Theme.popupSurface
    property real cornerRadius: Theme.cardRadius
    property int contentPadding: Theme.padding
    property real revealOriginX: card.width / 2
    property real revealOriginY: 0
    property real revealScale: 1
    property real initialRevealScale: Theme.popupInitialScale
    property int closeMotion: Theme.popupCloseMotion
    property int closeEasing: Easing.OutCubic
    readonly property real panelX: card.x
    readonly property real panelY: card.y
    readonly property real contentRadius: Theme.insetRadius(cornerRadius, contentPadding)
    property int popupWidth: 320
    property int popupHeight: body.childrenRect.height + contentPadding * 2
    signal aboutToOpen()
    property bool retained: false

    onVisibleChanged: {
        reveal.stop();
        dismiss.stop();
        if (motionSource) {
            retained = visible || motionSource.retained;
            return;
        }
        if (visible) {
            aboutToOpen();
            if (!retained) {
                revealScale = Theme.reducedMotion ? 1 : root.initialRevealScale;
                card.opacity = Theme.reducedMotion ? 1 : 0;
            }
            dismissWindow.visible = !root.hostItem && root.keyboardInteractive;
            retained = true;
            reveal.start();
            if (keyboardInteractive) contentItem.forceActiveFocus();
        } else if (retained) {
            // Stopping reveal freezes its scale, including an interrupted open.
            // Only opacity changes during dismissal.
            dismiss.start();
        }
    }
    function setRevealOrigin(x, y) {
        const point = card.mapFromItem(root.contentItem, x, y);
        revealOriginX = point.x;
        revealOriginY = point.y;
    }
    ParallelAnimation {
        id: reveal
        NumberAnimation { target: root; property: "revealScale"; to: 1; duration: Theme.popupOpenMotion; easing.type: Easing.OutCubic }
        NumberAnimation { target: card; property: "opacity"; to: 1; duration: Theme.popupOpenMotion; easing.type: Easing.OutCubic }
    }
    NumberAnimation {
        id: dismiss
        target: card
        property: "opacity"
        to: 0
        duration: root.closeMotion
        easing.type: root.closeEasing
        onFinished: if (!root.visible) {
            root.retained = false;
            dismissWindow.visible = false;
        }
    }

    // Outside clicks dismiss without focusing a menu that is about to close.
    // Keep this below the card's focusable surface, which only accepts card input.
    PanelWindow {
        id: dismissWindow
        screen: window.screen
        visible: false
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-popup-dismiss"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors { top: true; bottom: true; left: true; right: true }
        mask: Region {
            width: root.visible ? dismissWindow.width : 0
            height: root.visible ? dismissWindow.height : 0
            // Pointer entry can raise the dismissal surface above the card.
            // Its input region must never include the menu, regardless of stacking.
            Region {
                intersection: Intersection.Subtract
                x: card.x
                y: card.y
                width: card.width
                height: card.height
            }
        }
        contentItem.enabled: root.visible
        MouseArea { parent: root.hostItem ? root.contentItem : dismissWindow.contentItem; anchors.fill: parent; visible: root.retained; enabled: root.visible && root.dismissOnOutsideClick; z: 5; acceptedButtons: Qt.AllButtons; onClicked: root.visible = false }
    }

    // A layer surface avoids native xdg-popup grabs on layer-shell parents.
    PanelWindow {
        id: window
        visible: (root.retained || root.keepWindowAlive) && !root.hostItem
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "gnoblin-shell-popup"
        // Menus must receive arrows and Escape immediately, before any menu click.
        // The separate outside-click surface never takes keyboard focus.
        WlrLayershell.keyboardFocus: root.visible && root.keyboardInteractive ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        anchors { top: true; bottom: true; left: true; right: true }
        mask: Region {
            x: card.x
            y: card.y
            width: root.visible ? card.width : 0
            height: root.visible ? card.height : 0
        }
        contentItem.enabled: root.visible
        contentItem.Keys.onEscapePressed: root.visible = false
        Rectangle {
            id: card
            parent: root.contentItem
            visible: root.retained
            enabled: root.visible
            z: 6
            x: Math.max(root.placementLeft + Theme.gap, Math.min(root.preferredX, root.placementRight - width - Theme.gap))
            y: Math.max(Theme.gap, Math.min(root.preferredY, root.height - height - Theme.gap))
            width: Math.max(0, Math.min(root.popupWidth, root.placementRight - root.placementLeft - Theme.gap * 2))
            height: Math.min(root.popupHeight, root.height - Theme.gap * 2)
            radius: root.cornerRadius
            color: root.surfaceVisible ? root.surfaceColor : "transparent"
            opacity: root.motionSource ? root.motionSource.body.parent.opacity : 0
            border.color: Theme.outline
            border.width: root.surfaceVisible ? 1 : 0
            PanelOutline { surface: card }
            transform: Scale {
                origin.x: root.motionSource ? root.motionSource.panelX + root.motionSource.revealOriginX - card.x : root.revealOriginX
                origin.y: root.motionSource ? root.motionSource.panelY + root.motionSource.revealOriginY - card.y : root.revealOriginY
                xScale: root.motionSource ? root.motionSource.revealScale : root.revealScale
                yScale: root.motionSource ? root.motionSource.revealScale : root.revealScale
            }
            MouseArea { anchors.fill: parent }
            Item {
                id: body
                anchors.fill: parent
                anchors.margins: root.contentPadding
            }
        }
    }
}
