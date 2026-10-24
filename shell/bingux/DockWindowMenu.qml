import QtQuick

TrayMenu {
    id: root

    // DockWindowActions: resolves the exact Gnoblin window and dispatches.
    required property var bridge
    property var toplevel: null
    property point requestedPosition: Qt.point(0, 0)
    property int requestGeneration: 0

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

    // Opens the menu for one live window. A dispatch failure reopens it with
    // the error at the top, against a fresh lookup of the window's state.
    function openFor(target, x, y, failure) {
        const generation = ++requestGeneration;
        toplevel = target;
        requestedPosition = Qt.point(x, y);
        visible = false;
        bridge.lookup(target, (match, reason) => {
            if (generation !== root.requestGeneration)
                return;
            const notices = [failure, match ? "" : reason].filter(text => !!text);
            root.actions = root.buildActions(match, notices);
            root.visible = true;
        });
    }

    function run(method, arguments_) {
        bridge.dispatch(method, arguments_, error => {
            if (error)
                root.openFor(root.toplevel, root.requestedPosition.x, root.requestedPosition.y, error);
        });
    }

    // Lists only the actions valid for the window's current state. Move and
    // Resize are omitted: they need a dock-originated focus token that the
    // Gnoblin bridge does not expose yet.
    function buildActions(match, notices) {
        const result = notices.map(text => ({
                    text,
                    enabled: false,
                    isSeparator: false,
                    hasChildren: false,
                    checkState: Qt.Unchecked,
                    triggered: () => {}
                }));
        if (!match)
            return result;

        const record = match.record;
        const id = match.id;
        const append = (text, method, arguments_) => result.push({
                text,
                enabled: true,
                isSeparator: false,
                hasChildren: false,
                checkState: Qt.Unchecked,
                triggered: () => root.run(method, arguments_)
            });
        const separator = () => {
            if (result.length && !result[result.length - 1].isSeparator)
                result.push({
                    text: "",
                    enabled: false,
                    isSeparator: true,
                    hasChildren: false,
                    checkState: Qt.Unchecked
                });
        };

        const minimised = record.minimized === true;
        const maximised = record.maximized === true;
        const fullscreen = record.fullscreen === true;

        if (notices.length)
            separator();
        if (minimised)
            append("Restore", "window.unminimize", {
                id
            });
        else if (record.minimizable !== false)
            append("Minimise", "window.minimize", {
                id
            });
        if (!minimised) {
            if (maximised)
                append("Unmaximise", "window.set_maximized", {
                    id,
                    enabled: false
                });
            else if (record.maximizable !== false)
                append("Maximise", "window.set_maximized", {
                    id,
                    enabled: true
                });
            append(fullscreen ? "Exit fullscreen" : "Fullscreen", "window.set_fullscreen", {
                id,
                enabled: !fullscreen
            });
        }

        if (record.sticky !== true && match.workspaces.length > 1) {
            separator();
            const current = Number(record.workspace_number);
            for (const workspace of match.workspaces) {
                const number = Number(workspace.number);
                if (number !== current)
                    append("Move to " + (workspace.name || "Workspace " + number), "window.move_to_workspace", {
                        id,
                        workspace: {
                            number
                        }
                    });
            }
        }

        if (record.closable !== false) {
            separator();
            append("Close", "window.close", {
                id
            });
        }
        while (result.length && result[result.length - 1].isSeparator)
            result.pop();
        return result;
    }
}
