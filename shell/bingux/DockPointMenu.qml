import QtQuick

// A dock menu opened at a point on the screen: the window menu and the folder
// menu. The point is in screen coordinates, because the popup is screen-sized.
TrayMenu {
    id: root

    property point requestedPosition: Qt.point(0, 0)

    // Keep the row point that opened the menu outside its card, as WindowMenu
    // does, so a click near a screen edge cannot land on the card.
    readonly property real edgeGap: 1
    readonly property real menuX: requestedPosition.x + popupWidth + edgeGap <= width - edgeGap ? requestedPosition.x + edgeGap : requestedPosition.x - popupWidth - edgeGap
    readonly property real menuY: requestedPosition.y + popupHeight + edgeGap <= height - edgeGap ? requestedPosition.y + edgeGap : requestedPosition.y - popupHeight - edgeGap
    preferredX: menuX
    preferredY: menuY
    compactRows: true
    openMotion: 80
    revealOriginX: 0
    revealOriginY: 0
    onAboutToOpen: revealScale = Theme.reducedMotion ? 1 : initialRevealScale

    // One menu row. Rows are plain objects: TrayMenu reads these fields.
    function row(text, triggered, enabled) {
        return {
            text,
            enabled: enabled !== false,
            isSeparator: false,
            hasChildren: false,
            checkState: Qt.Unchecked,
            triggered
        };
    }
    function separator() {
        return {
            text: "",
            enabled: false,
            isSeparator: true,
            hasChildren: false,
            checkState: Qt.Unchecked
        };
    }
}
