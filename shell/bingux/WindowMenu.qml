import Quickshell

TrayMenu {
    id: root

    property string windowId: ""
    property string menuContext: ""
    property var windowSnapshot: []
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
    onVisibleChanged: if (!visible)
        menuContext = ""

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
        id: compositor
        enabled: CompositorEnvironment.gnoblin
        trackWindows: CompositorEnvironment.gnoblin
        trackWindowMenu: CompositorEnvironment.gnoblin
        onWindowMenuRequested: request => root.showNativeRequest(request)
        onWindowSnapshot: windows => {
            root.windowSnapshot = windows;
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
        menuContext = "";
        screen = target;
        windowId = request.window;
        requestedPosition = Qt.point(request.x - target.x, request.y - target.y);
        const allowed = ["minimize", "maximize", "unmaximize", "above", "unabove", "stick", "unstick", "close"];
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

    function showNativeRequest(request) {
        if (!request || request.event !== "gnoblin.window.menu-requested" || request.menu_type !== "wm" || typeof request.window_id !== "string" || !/^\d+$/.test(request.window_id) || !Number.isFinite(request.x) || !Number.isFinite(request.y))
            return;
        const targetWindow = windowSnapshot.find(window => String(window.id) === request.window_id);
        if (!targetWindow)
            return;
        const targetScreen = Quickshell.screens.find(s => request.x >= s.x && request.y >= s.y && request.x < s.x + s.width && request.y < s.y + s.height);
        if (!targetScreen)
            return;
        visible = false;
        screen = targetScreen;
        windowId = request.window_id;
        menuContext = typeof request.menu_context === "string" ? request.menu_context : "";
        requestedPosition = Qt.point(request.x - targetScreen.x, request.y - targetScreen.y);

        const nativeActions = [];
        const addAction = (text, action, enabled) => nativeActions.push({
                text,
                enabled: enabled !== false,
                isSeparator: false,
                hasChildren: false,
                checkState: Qt.Unchecked,
                triggered: () => root.triggerNativeAction(action)
            });
        if (targetWindow.minimizable !== false)
            addAction("Minimize", "minimize");
        if (targetWindow.maximizable !== false)
            addAction(targetWindow.maximized === true ? "Restore" : "Maximize", targetWindow.maximized === true ? "unmaximize" : "maximize");
        if (menuContext) {
            addAction("Move", "interactive-move");
            addAction("Resize from bottom right", "interactive-resize");
        }
        nativeActions.push({
            isSeparator: true
        });
        addAction(targetWindow.above === true ? "Unpin from Top" : "Always on Top", targetWindow.above === true ? "unabove" : "above");
        addAction(targetWindow.sticky === true ? "Move to This Workspace" : "Show on All Workspaces", targetWindow.sticky === true ? "unstick" : "stick");
        if (targetWindow.closable !== false)
            addAction("Close", "close");
        actions = nativeActions;
        visible = true;
    }

    function triggerNativeAction(action) {
        const targetId = windowId;
        const context = menuContext;
        // Drop the local copy before sending so no second activation can reuse it.
        menuContext = "";
        visible = false;
        compositor.requestWindowMenuAction(action, targetId, context, "south_east");
    }
}
