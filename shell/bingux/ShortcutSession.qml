import QtQuick
import Quickshell
import Quickshell.Io

// Reusable shortcut transport. No process is started when a key is pressed.
QtObject {
    id: root
    property var bindings: []
    property bool enabled: true
    property int bindingRetries: 0
    property int reconnectDelay: 250
    property double lastStatusAt: 0
    property int boundCount: 0
    property int sessionSerial: 0
    property var capabilities: []
    property bool trackWindows: false
    property bool trackPrivacy: false
    property bool trackWindowDrag: false
    property bool nativeProtocol: false
    property bool apiReady: false
    property int apiMajor: 0
    property int apiMinor: 0
    property int requestSequence: 0
    property var ownedBindings: []
    property bool bindingRequestPending: false
    property var apiRequests: ({})
    signal uiState(string name, var state)
    signal uiCommand(string name, var command)
    signal windowDrag(var state)
    signal layerAnimationPolicy(var state)
    signal snapContext(var state)
    signal snapCompleted(var state)
    signal privacySnapshot(var state)
    readonly property bool ready: socket.connected && (nativeProtocol ? apiReady && ownedBindings.length === (enabled ? bindings.length : 0) : boundCount === bindings.length)
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

    function nextRequestId() {
        requestSequence++;
        return "bingux-" + Date.now().toString(36) + "-" + requestSequence.toString(36);
    }

    function requestApi(method, arguments_, callback) {
        if (!socket.connected || !apiReady)
            return "";
        const requestId = nextRequestId();
        const requests = Object.assign({}, apiRequests);
        requests[requestId] = {
            method,
            callback
        };
        apiRequests = requests;
        send({
            op: "api",
            api_version: {
                major: apiMajor,
                minor: apiMinor
            },
            id: requestId,
            method,
            arguments: arguments_ || {}
        });
        return requestId;
    }

    function handleApiReply(record) {
        const pending = apiRequests[record.id];
        if (!pending)
            return false;
        const requests = Object.assign({}, apiRequests);
        delete requests[record.id];
        apiRequests = requests;
        if (record.event === "reply") {
            if (pending.callback)
                pending.callback(record.result || {}, "");
        } else {
            const message = record.message || "Gnoblin API request failed";
            if (pending.callback)
                pending.callback(null, message);
            else
                failed(message);
            if (pending.method === "shortcut.bind")
                bindingRetry.restart();
        }
        return true;
    }

    function normaliseBinding(binding) {
        let hold = binding.hold === undefined ? 0 : binding.hold;
        if (hold === 0)
            hold = "none";
        else if (hold === 8)
            hold = "alt";
        else if (hold === 67108864)
            hold = "super";
        if (!["none", "super", "control", "alt"].includes(hold))
            throw new Error("Unsupported held modifier for shortcut " + binding.id);

        const accelerator = String(binding.accelerator || "");
        const captureInput = binding.capture_input === true || binding.captureInput === true || accelerator === "Super";
        const trigger = binding.trigger || (accelerator === "Super" ? "release" : "press");
        const args = {
            id: String(binding.id || ""),
            accelerator,
            hold,
            trigger,
            mode: binding.modal === true ? "modal" : "passive"
        };
        if (captureInput)
            args.capture_input = true;
        if (!args.id || !args.accelerator)
            throw new Error("Shortcut bindings require an ID and accelerator");
        if (accelerator === "Super" && (!captureInput || trigger !== "release" || hold !== "none"))
            throw new Error("Bare Super requires release-triggered input capture");
        return args;
    }

    function desiredNativeBindings() {
        if (!enabled)
            return [];
        return bindings.map(normaliseBinding);
    }

    function reconcileNativeBindings() {
        if (!nativeProtocol || !apiReady || bindingRequestPending)
            return;

        let desired;
        try {
            desired = desiredNativeBindings();
        } catch (error) {
            failed(String(error));
            return;
        }

        const stale = ownedBindings.find(current => {
            const next = desired.find(binding => binding.id === current.id);
            return !next || JSON.stringify(next) !== JSON.stringify(current);
        });
        if (stale) {
            bindingRequestPending = true;
            requestApi("shortcut.unbind", {
                id: stale.id
            }, (result, error) => {
                bindingRequestPending = false;
                if (error && !error.includes("not registered by this connection"))
                    failed(error);
                ownedBindings = ownedBindings.filter(binding => binding.id !== stale.id);
                boundCount = ownedBindings.length;
                reconcileNativeBindings();
            });
            return;
        }

        const missing = desired.find(binding => !ownedBindings.some(current => current.id === binding.id));
        if (missing) {
            bindingRequestPending = true;
            requestApi("shortcut.bind", missing, (result, error) => {
                bindingRequestPending = false;
                if (error) {
                    failed(error);
                    bindingRetry.restart();
                } else {
                    ownedBindings = ownedBindings.concat([missing]);
                    boundCount = ownedBindings.length;
                }
                reconcileNativeBindings();
            });
            return;
        }

        boundCount = ownedBindings.length;
    }

    function requestInputAnchor() {
        send({
            op: "bingux.input-anchor"
        });
    }

    function insertText(windowId, text) {
        send({
            op: "bingux.type-text",
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
        if (!nativeProtocol) {
            send({
                op: "end",
                session: sessionSerial
            });
        }
    }

    function registerLegacyBindings() {
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

    function registerNativeSubscriptions() {
        const events = ["gnoblin.shortcut.binding-activated", "gnoblin.shortcut.session.activated", "gnoblin.shortcut.session.key", "gnoblin.shortcut.session.ended"];
        send({
            op: "events",
            api_version: {
                major: apiMajor,
                minor: apiMinor
            },
            events
        });
        if (trackWindows)
            send({
                op: "windows",
                api_version: {
                    major: apiMajor,
                    minor: Math.min(apiMinor, 1)
                }
            });
    }

    function registerBindings() {
        if (!socket.connected)
            return;
        if (nativeProtocol) {
            reconcileNativeBindings();
            return;
        }
        registerLegacyBindings();
    }

    function reconnect(reason) {
        console.warn("bingux-shortcuts: " + reason + "; reconnecting");
        root.failed(reason);
        if (root.socket.connected)
            root.socket.connected = false;
        root.retry.restart();
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
            root.ownedBindings = [];
            root.bindingRequestPending = false;
            root.apiRequests = ({});
            root.apiReady = false;
            root.nativeProtocol = false;
            root.apiMajor = 0;
            root.apiMinor = 0;
            root.capabilities = [];
            root.bindingRetries = 0;
            root.bindingRetry.stop();
            if (!connected) {
                root.healthCheck.stop();
                root.lastStatusAt = 0;
                root.cancelled();
                root.retry.restart();
            }
        }
        onError: {
            root.cancelled();
            // QLocalSocket may report an error while its requested connected
            // state remains true. Drop that state so retry forces a new connect.
            if (connected)
                connected = false;
            root.retry.restart();
        }
        parser: SplitParser {
            onRead: function (data) {
                try {
                    const record = JSON.parse(data);
                    if (record.event === "hello") {
                        root.nativeProtocol = Number.isInteger(record.api_major) && Number.isInteger(record.api_minor);
                        root.capabilities = record.capabilities || record.features || [];
                        root.reconnectDelay = 250;
                        root.lastStatusAt = Date.now();
                        root.retry.stop();
                        if (root.nativeProtocol) {
                            if (record.api_major !== 1 || record.api_minor < 22 || !Array.isArray(record.methods) || !record.methods.includes("shortcut.bind") || !record.methods.includes("shortcut.unbind")) {
                                root.failed("Gnoblin API 1.22 or newer is required for held shortcuts");
                                root.socket.connected = false;
                                return;
                            }
                            root.apiMajor = 1;
                            root.apiMinor = Math.min(record.api_minor, 63);
                            root.apiReady = true;
                            root.registerNativeSubscriptions();
                        }
                        root.registerBindings();
                        root.healthCheck.start();
                    } else if (record.event === "windows") {
                        root.windowSnapshot(record.windows || []);
                    } else if (record.event === "reply" || record.event === "error") {
                        if (root.handleApiReply(record)) {
                            root.lastStatusAt = Date.now();
                        } else if (record.id === root.healthRequestId) {
                            root.healthRequestId = "";
                            if (record.event === "reply" && record.result?.pong === "pong")
                                root.lastStatusAt = Date.now();
                            else if (record.event === "error")
                                root.failed(record.message || "Compositor ping failed");
                        } else if (record.event === "error") {
                            const message = record.message || "Compositor request failed";
                            root.failed(message);
                            if (String(message).startsWith("shortcut already claimed:") && !root.ready)
                                root.bindingRetry.restart();
                        }
                    } else if (record.event === "status") {
                        root.lastStatusAt = Date.now();
                        const activeBindings = record.bindings || [];
                        const missing = root.enabled ? root.bindings.filter(binding => !activeBindings.includes(binding.id)) : [];
                        if (missing.length > 0 && !root.bindingRetry.running) {
                            const ids = missing.map(binding => binding.id).join(", ");
                            console.warn("bingux-shortcuts: compositor lost bindings " + ids + "; registering again");
                            root.registerBindings();
                        }
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
                    else if (record.event === "pointer")
                        root.pointerPressed(record.x, record.y, record.button);
                    else if (record.event === "preview")
                        root.previewReceived(record.window, record.source, record.message || "");
                    else if (root.nativeProtocol && record.event === "gnoblin.shortcut.binding-activated") {
                        root.sessionSerial = record.session_id || 0;
                        root.activated(record.id, record.first !== false, record.modifiers || 0);
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.activated") {
                        if (record.first === false)
                            root.activated(record.id, false, record.modifiers || 0);
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.key") {
                        if (record.phase === "press")
                            root.keyPressed(record.keyval, record.modifiers || 0);
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.ended") {
                        if (record.reason === "released")
                            root.released();
                        else
                            root.cancelled();
                    }
                } catch (error) {
                    root.failed(String(error));
                }
            }
        }
    }
    property string healthRequestId: ""
    property var retry: Timer {
        interval: root.reconnectDelay
        onTriggered: {
            if (!CompositorEnvironment.gnoblin)
                return;
            root.reconnectDelay = Math.min(5000, root.reconnectDelay * 2);
            if (root.socket.connected) {
                root.socket.connected = false;
                Qt.callLater(() => {
                    if (CompositorEnvironment.gnoblin)
                        root.socket.connected = true;
                });
            } else {
                root.socket.connected = true;
            }
        }
    }
    property var healthCheck: Timer {
        interval: 5000
        repeat: true
        onTriggered: {
            if (!root.socket.connected)
                return;
            if (Date.now() - root.lastStatusAt > 15000) {
                root.reconnect("compositor status heartbeat timed out");
                return;
            }
            if (root.nativeProtocol) {
                root.healthRequestId = root.nextRequestId();
                root.send({
                    op: "ping",
                    id: root.healthRequestId
                });
            } else {
                root.send({
                    op: "status"
                });
            }
        }
    }
}
