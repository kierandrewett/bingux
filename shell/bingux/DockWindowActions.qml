import QtQml

// Resolves a Quickshell toplevel to its Gnoblin window and dispatches window
// API calls against that exact Gnoblin ID. Quickshell toplevels carry no
// Gnoblin ID, so the match is made from window.list and ambiguity is reported
// rather than guessed: an action must never reach a different window.
QtObject {
    id: root

    ShortcutSession {
        id: compositor
        enabled: true
    }

    function normaliseAppId(value) {
        return String(value || "").toLowerCase().replace(/\.desktop$/, "");
    }

    // callback(match, reason): match is {id, record, workspaces} or null with
    // a reason. Workspaces are used only for "Move to Workspace" entries.
    function lookup(toplevel, callback) {
        const requestId = compositor.requestApi("window.list", {}, (result, error) => {
            if (error) {
                callback(null, "Could not read windows: " + error);
                return;
            }
            const windows = result && Array.isArray(result.windows) ? result.windows : [];
            const appId = root.normaliseAppId(toplevel.appId);
            let matches = windows.filter(window => root.normaliseAppId(window.app_id) === appId && window.title === toplevel.title);
            const narrow = predicate => {
                const next = matches.filter(predicate);
                if (next.length)
                    matches = next;
            };
            narrow(window => window.focused === !!toplevel.activated);
            narrow(window => window.minimized === !!toplevel.minimized);
            narrow(window => window.maximized === !!toplevel.maximized);
            narrow(window => window.fullscreen === !!toplevel.fullscreen);
            if (matches.length === 0) {
                callback(null, "Window is no longer open");
                return;
            }
            if (matches.length > 1) {
                callback(null, "Window matches more than one Gnoblin window");
                return;
            }
            const record = matches[0];
            const finish = workspaces => callback({
                    id: String(record.id),
                    record,
                    workspaces
                }, "");
            const workspaceRequest = compositor.requestApi("workspace.list", {}, (workspaceResult, workspaceError) => {
                finish(!workspaceError && workspaceResult && Array.isArray(workspaceResult.workspaces) ? workspaceResult.workspaces : []);
            });
            if (!workspaceRequest)
                finish([]);
        });
        if (!requestId)
            Qt.callLater(callback, null, "Gnoblin is not ready");
    }

    // callback(error): error is "" when Gnoblin accepted the call.
    function dispatch(method, arguments_, callback) {
        const requestId = compositor.requestApi(method, arguments_, (result, error) => {
            callback(error ? method + " failed: " + error : "");
        });
        if (!requestId)
            Qt.callLater(callback, "Gnoblin is not ready for " + method);
    }
}
