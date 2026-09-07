import QtQuick
import Quickshell
import Quickshell.Io

// Reusable shortcut transport. No process is started when a key is pressed.
QtObject {
    id: root
    property var bindings: []
    property bool enabled: true
    property int boundCount: 0
    property bool trackWindows: false
    readonly property bool ready: socket.connected && boundCount === bindings.length
    signal activated(string id, bool first, int modifiers)
    signal keyPressed(int key, int modifiers)
    signal released()
    signal cancelled()
    signal failed(string message)
    signal windowSnapshot(var windows)
    signal pointerPressed(real x, real y, int button)
    function activateWindow(id) { send({op: "activate", window: id}); }
    function send(record) {
        if (!socket.connected) return;
        socket.write(JSON.stringify(record) + "\n");
        socket.flush();
    }
    function end() { send({op: "end"}); }
    function registerBindings() {
        boundCount = 0;
        send({op: "clear"});
        if (trackWindows) send({op: "windows"});
        if (enabled) for (const binding of bindings) send(Object.assign({op: "bind"}, binding));
    }
    onEnabledChanged: registerBindings()
    onBindingsChanged: registerBindings()
    property var socket: Socket {
        path: Quickshell.env("GNOBLIN_COMPOSITOR_SOCKET") || Quickshell.env("XDG_RUNTIME_DIR") + "/gnoblin/compositor-v1.sock"
        Component.onCompleted: connected = true
        onConnectedChanged: {
            root.boundCount = 0;
            if (!connected) { root.cancelled(); retry.restart(); }
        }
        onError: { root.cancelled(); retry.restart(); }
        parser: SplitParser {
            onRead: function(data) {
                try {
                    const record = JSON.parse(data);
                    if (record.event === "hello") root.registerBindings();
                    else if (record.event === "bound") root.boundCount++;
                    else if (record.event === "activated") root.activated(record.id, record.first, record.modifiers);
                    else if (record.event === "key") root.keyPressed(record.key, record.modifiers);
                    else if (record.event === "released") root.released();
                    else if (record.event === "cancelled") root.cancelled();
                    else if (record.event === "windows") root.windowSnapshot(record.windows);
                    else if (record.event === "pointer") root.pointerPressed(record.x, record.y, record.button);
                    else if (record.event === "error") root.failed(record.message);
                } catch (error) { root.failed(String(error)); }
            }
        }
    }
    property var retry: Timer {
        interval: 500
        onTriggered: if (!socket.connected) socket.connected = true
    }
}
