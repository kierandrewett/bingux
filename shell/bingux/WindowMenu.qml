import Quickshell

TrayMenu {
    id: root

    property string windowId: ""
    property string lastMenuRequestType: ""
    property point requestedPosition: Qt.point(0, 0)
    property int requestGeneration: 0
    signal requestFailed(string message)

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
        id: compositor
        enabled: true
        trackWindowMenu: true
        onWindowMenuRequested: request => root.openNativeRequest(request)
        onFailed: message => root.reportFailure(message)
    }

    function reportFailure(message) {
        console.warn("bingux-window-menu: " + message);
        requestFailed(message);
    }

    function nativeActions(window, menuContext) {
        const id = String(window.id);
        const result = [];
        const append = (text, method, arguments_, enabled, checked) => result.push({
                text,
                enabled: enabled !== false,
                isSeparator: false,
                hasChildren: false,
                checkState: checked ? Qt.Checked : Qt.Unchecked,
                triggered: () => root.requestWindowAction(method, arguments_)
            });

        if (window.minimized === true || window.maximized === true)
            append("Restore", "window.restore", {
                id
            }, true, false);
        if (menuContext && window.movable !== false)
            append("Move", "window.begin_move", {
                menu_context: menuContext
            }, true, false);
        if (menuContext && window.resizable !== false)
            append("Resize from bottom-right", "window.begin_resize", {
                menu_context: menuContext,
                edge: "south_east"
            }, true, false);
        if (window.minimized !== true)
            append("Minimise", "window.minimize", {
                id
            }, window.minimizable !== false, false);
        if (window.maximized !== true)
            append("Maximise", "window.set_maximized", {
                id,
                enabled: true
            }, window.maximizable !== false, false);

        result.push({
            text: "",
            enabled: false,
            isSeparator: true,
            hasChildren: false,
            checkState: Qt.Unchecked
        });
        append("Always on Top", "window.set_above", {
            id,
            enabled: window.above !== true
        }, true, window.above === true);
        append("On All Workspaces", "window.set_sticky", {
            id,
            enabled: window.sticky !== true
        }, true, window.sticky === true);
        append("Close", "window.close", {
            id
        }, window.closable !== false, false);
        return result;
    }

    function requestWindowAction(method, arguments_) {
        const requestId = compositor.requestApi(method, arguments_, (result, error) => {
            if (error)
                root.reportFailure(method + " failed: " + error);
        });
        if (!requestId)
            reportFailure("Gnoblin is not ready for " + method);
    }

    function openNativeRequest(request) {
        const generation = ++requestGeneration;
        lastMenuRequestType = request && typeof request.menu_type === "string" ? request.menu_type : "";
        visible = false;
        windowId = "";
        actions = [];

        if (!request || request.menu_type === "app")
            return;
        if (request.menu_type !== "wm" || typeof request.window_id !== "string" || !request.window_id || !Number.isFinite(request.x) || !Number.isFinite(request.y)) {
            reportFailure("Gnoblin sent an invalid window-menu request");
            return;
        }

        const requestId = compositor.requestApi("window.list", {}, (result, error) => {
            if (generation !== root.requestGeneration)
                return;
            if (error) {
                root.reportFailure("Could not read the requested window: " + error);
                return;
            }

            const window = (Array.isArray(result.windows) ? result.windows : []).find(candidate => String(candidate.id) === request.window_id);
            if (!window) {
                root.reportFailure("The requested window no longer exists");
                return;
            }

            const target = Quickshell.screens.find(screen => request.x >= screen.x && request.y >= screen.y && request.x < screen.x + screen.width && request.y < screen.y + screen.height);
            if (!target) {
                root.reportFailure("The requested window-menu screen is no longer available");
                return;
            }

            root.screen = target;
            root.windowId = request.window_id;
            root.requestedPosition = Qt.point(request.x - target.x, request.y - target.y);
            root.actions = root.nativeActions(window, typeof request.menu_context === "string" ? request.menu_context : "");
            root.visible = true;
        });
        if (!requestId)
            reportFailure("Gnoblin is not ready to list windows");
    }
}
