pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root
    property int sequence: 0
    property var queue: []
    property var completion: null
    property var request: Process {
        onExited: {
            const callback = root.completion;
            root.completion = null;
            try {
                if (callback) callback();
            } finally {
                Qt.callLater(root.drain);
            }
        }
    }

    function enqueue(method, args, callback) {
        if (!CompositorEnvironment.gnoblin) {
            if (callback) callback();
            return;
        }
        queue.push({callback: callback, command: ["timeout", "2s", "gdbus", "call", "--session",
            "--dest", "org.gnoblin.LaunchFeedback",
            "--object-path", "/org/gnoblin/LaunchFeedback",
            "--method", "org.gnoblin.LaunchFeedback." + method].concat(args)});
        drain();
    }

    function drain() {
        if (request.running || queue.length === 0)
            return;
        const next = queue.shift();
        completion = next.callback;
        request.command = next.command;
        request.running = true;
    }

    function begin(application, callback) {
        const token = "quickshell-" + Date.now() + "-" + (++sequence);
        // Register before dispatch so fast applications cannot map first.
        enqueue("Begin", [token, application || "", String(Theme.launchTimeout)], callback);
        return token;
    }

    function end(token) {
        if (token)
            enqueue("End", [token]);
    }
}
