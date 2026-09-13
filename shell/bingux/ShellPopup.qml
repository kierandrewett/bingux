import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Wayland

// `visible` is the requested state. The layer window stays mapped only long
// enough to finish closing, with input released as soon as closing starts.
Scope {
    id: root
    property bool windowShadow: false
    default property alias contents: body.data
    property bool visible: false
    property bool keepWindowAlive: false
    // Keep the card and its delegates resident while hidden. The input mask
    // still drops to zero when invisible, so this is a presentation cache only.
    property bool keepContentAlive: false
    property bool keyboardInteractive: true
    property bool pointerInteractive: true
    property bool dismissOnOutsideClick: true
    property alias screen: window.screen
    readonly property alias nativeWindow: window
    // Floating windows have no layer-surface margins. Keep their menus in
    // the same native window so Wayland does not need global coordinates.
    property Item hostItem: anchorWindow && !("anchors" in anchorWindow) ? anchorWindow.contentItem : null
    property var anchorWindow: null
    property Item anchorItem: null
    readonly property int popupDepth: {
        for (let item = anchorItem; item; item = item.parent) {
            if (item === card)
                return 0;
            if ("shellPopupDepth" in item)
                return item.shellPopupDepth + 1;
        }
        return 0;
    }
    property int anchorAlignment: Qt.AlignRight
    readonly property real placementLeft: !hostItem && anchorWindow ? anchorWindow.margins.left : 0
    readonly property real placementRight: width - (!hostItem && anchorWindow ? anchorWindow.margins.right : 0)
    readonly property real belowAnchorY: anchorItem ? anchorPosition.y + Theme.gap : Theme.barHeight + Theme.gap
    readonly property bool anchorAbove: !hostItem && !!anchorWindow && anchorWindow.anchors.bottom && !anchorWindow.anchors.top
    readonly property real anchorTop: anchorAbove && typeof anchorWindow.popupAnchorTop === "number" ? anchorWindow.popupAnchorTop : anchorPosition.y - (anchorItem ? anchorItem.height : 0)
    readonly property point anchorPosition: {
        // Reparenting a launcher must not anchor a popup to its own content.
        if (!anchorItem || anchorItem.Window.window === window.contentItem.Window.window)
            return Qt.point(0, Theme.barHeight);
        // Track layout and reparenting, including controls moved into overflow.
        DesktopEditing.observeGeometry(anchorItem);
        const x = anchorAlignment === Qt.AlignHCenter ? anchorItem.width / 2 : anchorItem.width;
        if (hostItem)
            return anchorItem.mapToItem(hostItem, x, anchorItem.height);
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
        function onRetainedChanged() {
            if (!root.motionSource.retained)
                root.retained = false;
        }
    }
    readonly property real width: hostItem ? hostItem.width : window.width
    readonly property real height: hostItem ? hostItem.height : window.height
    readonly property var contentItem: hostItem ? hostItem : window.contentItem
    readonly property alias body: body
    property alias presentationOpacity: card.opacity
    readonly property bool closing: retained && !visible
    property real preferredX: anchorItem ? anchorPosition.x - popupWidth / (anchorAlignment === Qt.AlignHCenter ? 2 : 1) : (width - popupWidth) / 2
    property real preferredY: anchorItem ? (anchorAbove ? anchorTop - popupHeight - Theme.gap : anchorPosition.y + Theme.gap) : Theme.barHeight + Theme.gap
    property bool surfaceVisible: true
    property color surfaceColor: Theme.popupSurface
    property real cornerRadius: Theme.cardRadius
    property int contentPadding: Theme.popupPadding
    property real revealOriginX: card.width / 2
    property real revealOriginY: 0
    property real revealScale: 1
    property real initialRevealScale: Theme.popupInitialScale
    property int openMotion: Theme.popupOpenMotion
    property int closeMotion: Theme.popupCloseMotion
    property int closeEasing: Easing.OutCubic
    readonly property real panelX: card.x
    readonly property real panelY: card.y
    readonly property real contentRadius: Theme.insetRadius(cornerRadius, contentPadding)
    property int popupWidth: 320
    property int popupHeight: body.childrenRect.height + contentPadding * 2
    signal aboutToOpen
    property bool retained: false
    readonly property bool compositorClose: !hostItem && !keepWindowAlive && !motionSource && PopupTransitions.matches(closeMotion, closeEasing)
    readonly property bool compositorOpen: !hostItem && !keepWindowAlive && PopupTransitions.fadesIn()

    onVisibleChanged: {
        reveal.stop();
        dismiss.stop();
        if (motionSource) {
            retained = visible || motionSource.retained;
            return;
        }
        if (visible) {
            PopupTransitions.refresh();
            aboutToOpen();
            if (!retained) {
                revealScale = Theme.reducedMotion ? 1 : root.initialRevealScale;
                card.opacity = Theme.reducedMotion || compositorOpen ? 1 : 0;
            }
            dismissWindow.visible = !root.hostItem && root.keyboardInteractive;
            retained = true;
            reveal.start();
            if (keyboardInteractive)
                contentItem.forceActiveFocus();
        } else if (retained) {
            // The last committed buffer already contains the current entrance
            // opacity and scale. Freeze it, then let the compositor fade it once.
            if (compositorClose) {
                retained = false;
                dismissWindow.visible = false;
            } else {
                dismiss.start();
            }
        }
    }
    function setRevealOrigin(x, y) {
        const point = card.mapFromItem(root.contentItem, x, y);
        revealOriginX = point.x;
        revealOriginY = point.y;
    }
    ParallelAnimation {
        id: reveal
        NumberAnimation {
            target: root
            property: "revealScale"
            to: 1
            duration: root.openMotion
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: card
            property: "opacity"
            to: 1
            duration: root.openMotion
            easing.type: Easing.OutCubic
        }
    }
    NumberAnimation {
        id: dismiss
        target: card
        property: "opacity"
        to: 0
        duration: root.closeMotion
        easing.type: root.closeEasing
        onFinished: if (!root.visible) {
            if (!root.keepContentAlive)
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
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
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
        MouseArea {
            parent: root.hostItem ? root.contentItem : dismissWindow.contentItem
            anchors.fill: parent
            visible: root.retained
            enabled: root.visible && root.dismissOnOutsideClick
            z: 5 + root.popupDepth * 2
            acceptedButtons: Qt.AllButtons
            onClicked: root.visible = false
        }
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
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        mask: Region {
            x: card.x
            y: card.y
            width: root.visible && root.pointerInteractive ? card.width : 0
            height: root.visible ? card.height : 0
        }
        contentItem.enabled: root.visible
        contentItem.Keys.onEscapePressed: root.visible = false
        Item {
            id: card
            BackgroundEffect {
                target: card
                radius: root.cornerRadius
                requested: root.surfaceVisible
            }
            SurfaceFade {
                target: card
                layerTarget: root.windowShadow ? composition : card
            }
            // Keep native context-menu events inside the popup, including
            // right-clicks handled by controls before this event arrives.
            ContextMenu.menu: null
            ContextMenu.onRequested: position => {}
            // Inline child menus share one scene with their parent popup.
            readonly property int shellPopupDepth: root.popupDepth
            readonly property var geometryRevision: [popupScale.xScale, popupScale.yScale, popupScale.origin.x, popupScale.origin.y]
            parent: root.contentItem
            visible: root.retained
            enabled: root.visible
            z: 6 + root.popupDepth * 2
            // Centred anchors can land between pixels; keep settled borders crisp.
            x: Math.round(Math.max(root.placementLeft + Theme.gap, Math.min(root.preferredX, root.placementRight - width - Theme.gap)))
            y: Math.round(Math.max(Theme.gap, Math.min(root.preferredY, root.height - height - Theme.gap)))
            width: Math.max(0, Math.min(root.popupWidth, root.placementRight - root.placementLeft - Theme.gap * 2))
            height: Math.min(root.popupHeight, root.height - Theme.gap * 2)
            opacity: root.motionSource ? root.motionSource.presentationOpacity : 0
            MouseArea {
                anchors.fill: parent
            }
            Item {
                id: composition
                x: -128
                y: -128
                width: card.width + 256
                height: card.height + 256
                Rectangle {
                    id: material
                    parent: root.windowShadow ? composition : card
                    x: root.windowShadow ? 128 : 0
                    y: x
                    width: card.width
                    height: card.height
                    radius: root.cornerRadius
                    color: root.surfaceVisible ? root.surfaceColor : "transparent"
                    border.color: Theme.outline
                    border.width: root.surfaceVisible ? 1 : 0
                    PanelOutline {
                        surface: material
                    }
                    Loader {
                        active: root.windowShadow && root.surfaceVisible
                        sourceComponent: PopupShadow {
                            surface: material
                        }
                    }
                }
                Item {
                    id: body
                    parent: root.windowShadow ? material : card
                    // Reparenting can append the material after this item.
                    // Keep content above it regardless of child insertion order.
                    z: 1
                    anchors.fill: parent
                    anchors.margins: root.contentPadding
                }
            }
            transform: Scale {
                id: popupScale
                origin.x: root.motionSource ? root.motionSource.panelX + root.motionSource.revealOriginX - card.x : root.revealOriginX
                origin.y: root.motionSource ? root.motionSource.panelY + root.motionSource.revealOriginY - card.y : root.revealOriginY
                xScale: root.motionSource ? root.motionSource.revealScale : root.revealScale
                yScale: root.motionSource ? root.motionSource.revealScale : root.revealScale
            }
        }
    }
}
