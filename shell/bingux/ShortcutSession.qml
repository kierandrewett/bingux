import QtQuick
import Quickshell
import Quickshell.Io

// Reusable shortcut transport. No process is started when a key is pressed.
QtObject {
    id: root
    property var bindings: []
    property bool enabled: true
    property int bindingRetries: 0
    property int boundCount: 0
    property int sessionSerial: 0
    property var capabilities: []
    property bool trackWindows: false
    property bool trackPrivacy: false
    property bool trackWindowDrag: false
    signal uiState(string name, var state)
    signal uiCommand(string name, var command)
    signal windowDrag(var state)
    signal layerAnimationPolicy(var state)
    signal snapContext(var state)
    signal snapCompleted(var state)
    signal privacySnapshot(var state)
    readonly property bool ready: socket.connected && boundCount === bindings.length
    readonly property bool connected: socket.connected
    signal activated(string id, bool first, int modifiers)
    signal keyPressed(int key, int modifiers)
    signal released
    signal cancelled
    signal failed(string message)
    signal windowSnapshot(var windows)
    signal pointerPressed(real x, real y, int button)
    signal previewReceived(string windowId, string source, string error)
    signal textInserted(string windowId)
    signal inputAnchor(var anchor)
    function requestInputAnchor() {
        send({
            op: "input-anchor"
        });
    }
    function insertText(windowId, text) {
        send({
            op: "type-text",
            window: windowId,
            text: text
        });
    }
    function requestPreview(id, width, height) {
        send({
            op: "preview",
            window: id,
            width: width,
            height: height
        });
    }
    function activateWindow(id) {
        send({
            op: "activate",
            window: id,
            session: sessionSerial
        });
    }
    function send(record) {
        if (!socket.connected)
            return;
        socket.write(JSON.stringify(record) + "\n");
        socket.flush();
    }
    function end() {
        send({
            op: "end",
            session: sessionSerial
        });
    }
    function registerBindings() {
        boundCount = 0;
        send({
            op: "clear"
        });
        if (trackWindows)
            send({
                op: "windows"
            });
        if (trackWindowDrag)
            send({
                op: "window-drag"
            });
        if (trackPrivacy)
            send({
                op: "privacy"
            });
        if (enabled)
            for (const binding of bindings)
                send(Object.assign({
                    op: "bind"
                }, binding));
    }
    onReadyChanged: if (ready) {
        bindingRetries = 0;
        bindingRetry.stop();
    }
    // During QML reload the old connection can still own the keys when the
    // replacement registers. Retry that transient conflict after it disconnects.
    property var bindingRetry: Timer {
        interval: Math.min(2000, 250 * Math.pow(2, root.bindingRetries))
        onTriggered: {
            root.bindingRetries++;
            root.registerBindings();
        }
    }
    onEnabledChanged: registerBindings()
    onBindingsChanged: registerBindings()
    property var socket: Socket {
        path: Quickshell.env("GNOBLIN_COMPOSITOR_SOCKET") || Quickshell.env("XDG_RUNTIME_DIR") + "/gnoblin/compositor-v1.sock"
        Component.onCompleted: connected = CompositorEnvironment.gnoblin
        onConnectedChanged: {
            root.boundCount = 0;
            root.capabilities = [];
            root.bindingRetries = 0;
            bindingRetry.stop();
            if (!connected) {
                root.cancelled();
                retry.restart();
            }
        }
        onError: {
            root.cancelled();
            retry.restart();
        }
        parser: SplitParser {
            onRead: function (data) {
                try {
                    const record = JSON.parse(data);
                    if (record.event === "hello") {
                        root.capabilities = record.features || [];
                        root.registerBindings();
                    } else if (record.event === "ui-state")
                        root.uiState(record.name, record.state);
                    else if (record.event === "ui-command")
                        root.uiCommand(record.name, record.command);
                    else if (record.event === "layer-animation-policy")
                        root.layerAnimationPolicy(record);
                    else if (record.event === "bound")
                        root.boundCount++;
                    else if (record.event === "activated") {
                        root.sessionSerial = record.session || 0;
                        root.activated(record.id, record.first, record.modifiers);
                    } else if (record.event === "key")
                        root.keyPressed(record.key, record.modifiers);
                    else if (record.event === "released")
                        root.released();
                    else if (record.event === "typed")
                        root.textInserted(record.window);
                    else if (record.event === "input-anchor")
                        root.inputAnchor(record);
                    else if (record.event === "cancelled")
                        root.cancelled();
                    else if (record.event === "window-drag")
                        root.windowDrag(record);
                    else if (record.event === "snap-completed")
                        root.snapCompleted(record);
                    else if (record.event === "snap-context")
                        root.snapContext(record);
                    else if (record.event === "privacy")
                        root.privacySnapshot(record);
                    else if (record.event === "windows")
                        root.windowSnapshot(record.windows);
                    else if (record.event === "pointer")
                        root.pointerPressed(record.x, record.y, record.button);
                    else if (record.event === "preview")
                        root.previewReceived(record.window, record.source, record.message || "");
                    else if (record.event === "error") {
                        root.failed(record.message);
                        if (String(record.message).startsWith("shortcut already claimed:") && root.bindingRetries < 8)
                            bindingRetry.restart();
                    }
                } catch (error) {
                    root.failed(String(error));
                }
            }
        }
    }
    property var retry: Timer {
        interval: 500
        onTriggered: if (CompositorEnvironment.gnoblin && !socket.connected)
            socket.connected = true
    }
}
