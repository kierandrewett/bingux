import Quickshell

TrayMenu {
    id: root

    property string windowId: ""
    property point requestedPosition: Qt.point(0, 0)
    // Keep the popup surface and its delegates warm between openings.
    keepWindowAlive: true
    keepContentAlive: true
    revealOriginX: 0
    revealOriginY: 0
    openMotion: 80

    // Cached reopenings still get a small top-left reveal instead of starting
    // at the completed scale left behind by the previous opening.
    onAboutToOpen: revealScale = Theme.reducedMotion ? 1 : initialRevealScale

    // Keep the titlebar point that opened the menu outside its input region.
    // This matters when the click is close to a frame edge: ShellPopup's
    // clamping can otherwise put the card back over the trigger point.
    // A one-pixel separation is enough to keep the stationary trigger point
    // outside the card without visibly moving the menu away from the frame.
    readonly property real edgeGap: 1
    readonly property real menuX: requestedPosition.x + popupWidth + edgeGap <= width - edgeGap ? requestedPosition.x + edgeGap : requestedPosition.x - popupWidth - edgeGap
    readonly property real menuY: requestedPosition.y + popupHeight + edgeGap <= height - edgeGap ? requestedPosition.y + edgeGap : requestedPosition.y - popupHeight - edgeGap
    preferredX: menuX
    preferredY: menuY
    compactRows: true

    ShortcutSession {
        enabled: root.visible
        trackWindows: root.visible
        onWindowSnapshot: windows => {
            if (root.visible && !windows.some(w => String(w.id) === root.windowId))
                root.visible = false;
        }
    }

    function showRequest(payload) {
        const request = JSON.parse(payload);
        if (request.version !== 1 || !/^\d+$/.test(request.window) || !Number.isFinite(request.x) || !Number.isFinite(request.y) || !Array.isArray(request.actions))
            throw new Error("Invalid window menu request");
        const target = Quickshell.screens.find(s => request.x >= s.x && request.y >= s.y && request.x < s.x + s.width && request.y < s.y + s.height);
        if (!target)
            throw new Error("Window menu screen is no longer available");
        visible = false;
        screen = target;
        windowId = request.window;
        requestedPosition = Qt.point(request.x - target.x, request.y - target.y);
        const allowed = ["minimize", "maximize", "unmaximize", "interactive-move", "interactive-resize", "above", "unabove", "stick", "unstick", "close"];
        actions = request.actions.filter(a => a.isSeparator || allowed.includes(a.id)).map(a => ({
                    text: a.text || "",
                    enabled: a.enabled === true,
                    isSeparator: a.isSeparator === true,
                    hasChildren: false,
                    checkState: a.checked ? Qt.Checked : Qt.Unchecked,
                    triggered: () => {
                        // Opening the popup changes focus: retain the original target.
                        Quickshell.execDetached(["gnoblinctl", "window", a.id, request.window]);
                    }
                }));
        visible = true;
    }
}
