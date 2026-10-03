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
    property bool trackWindowMenu: false
    property bool helloReceived: false
    property bool nativeProtocol: false
    property bool apiReady: false
    property int apiMajor: 0
    property int apiMinor: 0
    property int requestSequence: 0
    property var ownedBindings: []
    property bool bindingRequestPending: false
    property string activeSessionBindingId: ""
    property string activeSessionId: ""
    property string pendingEndSessionId: ""
    property var endingSessionIds: []
    property var apiRequests: ({})
    property var thumbnailRequests: ({})
    property var operationRequests: ({})
    signal uiState(string name, var state)
    signal uiCommand(string name, var command)
    signal windowDrag(var state)
    signal layerAnimationPolicy(var state)
    signal snapContext(var state)
    signal snapCompleted(var state)
    signal privacySnapshot(var state)
    readonly property bool ready: socket.connected && helloReceived && (nativeProtocol ? apiReady && ownedBindings.length === (enabled ? bindings.length : 0) : boundCount === bindings.length)
    readonly property bool connected: socket.connected
    signal activated(string id, bool first, int modifiers, string focusContext)
    signal keyPressed(int key, int modifiers, string focusContext)
    signal keyReleased(int key, int modifiers, string focusContext)
    signal released
    signal cancelled
    signal failed(string message)
    signal windowSnapshot(var windows)
    signal windowMenuRequested(var request)
    signal pointerPressed(real x, real y, int button)
    signal previewReceived(string windowId, string source, string error)
    signal textInserted(string windowId)
    signal textTargetReceived(var target, string error, string requestTag)
    signal textInsertionFinished(string windowId, bool succeeded, string error, string requestTag)
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

    function handleThumbnailCompletion(record) {
        const operationId = String(record.operation_id || "");
        const windowId = thumbnailRequests[operationId];
        if (!windowId)
            return false;
        const requests = Object.assign({}, thumbnailRequests);
        delete requests[operationId];
        thumbnailRequests = requests;
        const image = record.value?.data;
        if (record.ok === true && typeof image === "string" && image.length > 0)
            previewReceived(windowId, "data:image/png;base64," + image, "");
        else
            previewReceived(windowId, "", record.error?.message || "Gnoblin could not capture this window");
        return true;
    }

    function handleOperationCompletion(record) {
        if (handleThumbnailCompletion(record))
            return true;
        const operationId = String(record.operation_id || "");
        const pending = operationRequests[operationId];
        if (!pending)
            return false;
        const requests = Object.assign({}, operationRequests);
        delete requests[operationId];
        if (record.ok !== true)
            failed(record.error?.message || pending.method + " failed");
        return true;
    }

    function stopPrivacySessions(method, legacyOperation) {
        if (!nativeProtocol) {
            send({
                op: legacyOperation
            });
            return;
        }
        const requestId = requestApi(method, {}, (result, error) => {
            if (error) {
                failed(error);
                return;
            }
            const operationId = Number(result.operation_id ?? result.request_id);
            if (!Number.isSafeInteger(operationId) || operationId <= 0) {
                failed("Gnoblin returned no operation ID for " + method);
                return;
            }
            operationRequests = Object.assign({}, operationRequests, {
                [String(operationId)]: {
                    method
                }
            });
        });
        if (!requestId)
            failed("Gnoblin is not ready for " + method);
    }

    function stopSharing() {
        stopPrivacySessions("privacy.stop_sharing", "stop-sharing");
    }

    function stopRecording() {
        stopPrivacySessions("privacy.stop_recording", "stop-recording");
    }

    function nativeDragRecord(record, active) {
        const pointer = record.pointer || {};
        const modifiers = record.modifiers || {};
        return {
            nativeDrag: true,
            active,
            serial: record.id || record.drag_id,
            drag_id: record.id || record.drag_id,
            drag_token: record.drag_token || "",
            window: record.window_id || "",
            monitor: Object.assign({
                id: record.monitor_id || ""
            }, record.monitor || {}),
            area: record.work_area || {},
            x: pointer.x ?? record.pointer_x ?? 0,
            y: pointer.y ?? record.pointer_y ?? 0,
            modifiers: (modifiers.control ? 4 : 0) | (modifiers.shift ? 1 : 0),
            maximized: record.maximized === true
        };
    }

    function offerSnap(dragId, dragToken, targets) {
        if (!nativeProtocol) {
            send({
                op: "snap-offer",
                serial: dragId,
                regions: targets
            });
            return;
        }
        if (!Array.isArray(targets) || targets.length === 0)
            return;
        requestApi("window.snap.offer", {
            drag_id: Number(dragId),
            drag_token: String(dragToken || ""),
            targets
        }, (result, error) => {
            if (error)
                failed(error);
        });
    }

    function requestSnapContext(focusContext) {
        if (!nativeProtocol) {
            send({
                op: "snap-context"
            });
            return;
        }
        if (apiMinor < 28 || !focusContext) {
            failed("Keyboard snapping requires a recent shortcut context and Gnoblin API 1.28");
            return;
        }
        requestApi("window.snap_context", {
            focus_context: focusContext
        }, (result, error) => {
            if (error)
                failed(error);
            else {
                snapContext(Object.assign({}, result, {
                    window: result.window_id,
                    monitor: Object.assign({
                        id: result.monitor_id
                    }, result.monitor || {}),
                    area: result.work_area,
                    nativeContext: true
                }));
            }
        });
    }

    function commitSnap(context, monitorId, frame, legacyWindow) {
        if (!nativeProtocol) {
            send({
                op: "snap-window",
                window: legacyWindow,
                monitor: monitorId,
                target: frame
            });
            return;
        }
        requestApi("window.snap", {
            context,
            monitor_id: monitorId,
            frame
        }, (result, error) => {
            if (error)
                failed(error);
            else
                snapCompleted(result);
        });
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

    function resumePendingEnd() {
        if (!pendingEndSessionId)
            return false;
        const sessionId = pendingEndSessionId;
        pendingEndSessionId = "";
        if (sessionId !== activeSessionId)
            return false;
        end();
        return true;
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
                if (resumePendingEnd())
                    return;
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
                if (resumePendingEnd())
                    return;
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

    function requestTextTarget(focusContext, requestTag) {
        if (!nativeProtocol) {
            requestInputAnchor();
            return;
        }
        if (apiMinor < 28 || !focusContext) {
            textTargetReceived(null, "Text insertion needs a fresh shortcut context and Gnoblin API 1.28", requestTag || "");
            return;
        }
        const requestId = requestApi("input.text_target", {
            focus_context: focusContext
        }, (result, error) => {
            if (error) {
                textTargetReceived(null, error, requestTag || "");
                return;
            }
            textTargetReceived(result, "", requestTag || "");
        });
        if (!requestId)
            textTargetReceived(null, "Gnoblin is not ready for input.text_target", requestTag || "");
    }

    function insertText(windowId, text, target, requestTag) {
        if (nativeProtocol) {
            if (apiMinor < 28 || !target) {
                textInsertionFinished(windowId || "", false, "No valid Gnoblin text target is available", requestTag || "");
                return;
            }
            const requestId = requestApi("input.insert_text", {
                target,
                text
            }, (result, error) => {
                if (error) {
                    textInsertionFinished(windowId || "", false, error, requestTag || "");
                    return;
                }
                const succeeded = result.inserted === true;
                const message = succeeded ? "" : "Gnoblin did not confirm text insertion";
                textInsertionFinished(windowId || "", succeeded, message, requestTag || "");
            });
            if (!requestId)
                textInsertionFinished(windowId || "", false, "Gnoblin is not ready for input.insert_text", requestTag || "");
            return;
        }
        send({
            op: "bingux.type-text",
            window: windowId,
            text: text
        });
    }

    function requestPreview(id, width, height) {
        if (nativeProtocol) {
            if (!apiReady || apiMinor < 23) {
                previewReceived(id, "", "Window thumbnails require Gnoblin API 1.23 or newer");
                return;
            }
            const requestId = requestApi("window.thumbnail", {
                id,
                width,
                height
            }, (result, error) => {
                if (error) {
                    previewReceived(id, "", error);
                    return;
                }
                const operationId = Number(result.operation_id ?? result.request_id);
                if (!Number.isSafeInteger(operationId) || operationId <= 0) {
                    previewReceived(id, "", "Gnoblin returned no thumbnail operation ID");
                    return;
                }
                thumbnailRequests = Object.assign({}, thumbnailRequests, {
                    [String(operationId)]: id
                });
            });
            if (!requestId)
                previewReceived(id, "", "Gnoblin is not ready for thumbnail requests");
            return;
        }
        send({
            op: "preview",
            window: id,
            width: width,
            height: height
        });
    }

    function activateWindow(id, focusContext) {
        if (nativeProtocol) {
            if (!focusContext) {
                failed("Window focus requires a recent shortcut activation");
                return;
            }
            const requestId = requestApi("window.focus", {
                id: String(id),
                focus_context: focusContext
            }, (result, error) => {
                if (error)
                    failed(error);
            });
            if (!requestId)
                failed("Gnoblin is not ready to focus a window");
            return;
        }
        send({
            op: "activate",
            window: id,
            session: sessionSerial
        });
    }

    function requestWindowMenuAction(action, windowId, menuContext, edge) {
        if (!nativeProtocol || !apiReady) {
            failed("Gnoblin is not ready to handle this window menu action");
            return false;
        }
        let method = "";
        let arguments_ = {};
        if (action === "interactive-move" || action === "interactive-resize") {
            if (apiMinor < 30 || !menuContext) {
                failed("Interactive move and resize require a WM menu request from Gnoblin API 1.30 or newer");
                return false;
            }
            method = action === "interactive-move" ? "window.begin_move" : "window.begin_resize";
            arguments_ = {
                menu_context: menuContext
            };
            if (action === "interactive-resize")
                arguments_.edge = edge || "south_east";
        } else {
            const methods = {
                close: ["window.close",
                    {}
                ],
                minimize: ["window.minimize",
                    {}
                ],
                maximize: ["window.set_maximized",
                    {
                        enabled: true
                    }
                ],
                unmaximize: ["window.set_maximized",
                    {
                        enabled: false
                    }
                ],
                above: ["window.set_above",
                    {
                        enabled: true
                    }
                ],
                unabove: ["window.set_above",
                    {
                        enabled: false
                    }
                ],
                stick: ["window.set_sticky",
                    {
                        enabled: true
                    }
                ],
                unstick: ["window.set_sticky",
                    {
                        enabled: false
                    }
                ]
            };
            const call = methods[action];
            if (!call) {
                failed("Unknown window menu action: " + action);
                return false;
            }
            method = call[0];
            arguments_ = Object.assign({
                id: String(windowId)
            }, call[1]);
        }
        const requestId = requestApi(method, arguments_, (result, error) => {
            if (error)
                failed(error);
        });
        if (!requestId) {
            failed("Gnoblin is not ready to handle this window menu action");
            return false;
        }
        return true;
    }

    function send(record) {
        if (!socket.connected)
            return;
        socket.write(JSON.stringify(record) + "\n");
        socket.flush();
    }

    function end() {
        if (nativeProtocol) {
            const bindingId = activeSessionBindingId;
            const sessionId = activeSessionId;
            if (!bindingId || !sessionId)
                return;
            if (bindingRequestPending) {
                pendingEndSessionId = sessionId;
                return;
            }
            endingSessionIds = endingSessionIds.concat([sessionId]);
            bindingRequestPending = true;
            const requestId = requestApi("shortcut.unbind", {
                id: bindingId
            }, (result, error) => {
                bindingRequestPending = false;
                if (error) {
                    endingSessionIds = endingSessionIds.filter(id => id !== sessionId);
                    failed(error);
                    return;
                }
                ownedBindings = ownedBindings.filter(binding => binding.id !== bindingId);
                boundCount = ownedBindings.length;
                activeSessionBindingId = "";
                activeSessionId = "";
                sessionSerial = 0;
                reconcileNativeBindings();
            });
            if (!requestId) {
                bindingRequestPending = false;
                endingSessionIds = endingSessionIds.filter(id => id !== sessionId);
                failed("Gnoblin is not ready to end the shortcut session");
            }
        } else {
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
        const events = ["gnoblin.shortcut.binding-activated", "gnoblin.shortcut.session.activated", "gnoblin.shortcut.session.key", "gnoblin.shortcut.session.ended", "gnoblin.operation.completed"];
        if (trackPrivacy)
            events.push("gnoblin.privacy.changed");
        if (trackWindowDrag)
            events.push("gnoblin.window.drag.started", "gnoblin.window.drag.updated", "gnoblin.window.drag.ended");
        if (trackWindowMenu && apiMinor >= 27)
            events.push("gnoblin.window.menu-requested");
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
        if (trackPrivacy)
            requestApi("privacy.state", {}, (state, error) => {
                if (error)
                    failed(error);
                else
                    privacySnapshot(state);
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
            root.thumbnailRequests = ({});
            root.operationRequests = ({});
            root.helloReceived = false;
            root.apiReady = false;
            root.nativeProtocol = false;
            root.activeSessionBindingId = "";
            root.activeSessionId = "";
            root.pendingEndSessionId = "";
            root.endingSessionIds = [];
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
                            if (root.trackWindowDrag && (record.api_minor < 28 || !record.methods.includes("window.snap_context") || !record.methods.includes("window.snap") || !record.methods.includes("window.snap.offer"))) {
                                root.failed("Gnoblin API 1.28 or newer with snap methods is required for snapping");
                                root.socket.connected = false;
                                return;
                            }
                            if (root.trackWindowMenu && record.api_minor < 27) {
                                root.failed("Gnoblin API 1.27 or newer is required for native window menus");
                                root.socket.connected = false;
                                return;
                            }
                            root.apiMajor = 1;
                            root.apiMinor = Math.min(record.api_minor, 68);
                            root.apiReady = true;
                            root.registerNativeSubscriptions();
                        }
                        root.registerBindings();
                        root.healthCheck.start();
                        root.helloReceived = true;
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
                    } else if (root.nativeProtocol && record.event === "gnoblin.operation.completed") {
                        root.handleOperationCompletion(record);
                    } else if (root.nativeProtocol && record.event === "gnoblin.privacy.changed") {
                        root.privacySnapshot(record.state || {});
                    } else if (root.nativeProtocol && record.event === "gnoblin.window.menu-requested") {
                        root.windowMenuRequested(record);
                    } else if (root.nativeProtocol && record.event === "gnoblin.window.drag.started") {
                        root.windowDrag(root.nativeDragRecord(record, true));
                    } else if (root.nativeProtocol && record.event === "gnoblin.window.drag.updated") {
                        root.windowDrag(root.nativeDragRecord(record, true));
                    } else if (root.nativeProtocol && record.event === "gnoblin.window.drag.ended") {
                        root.windowDrag(root.nativeDragRecord(record, false));
                        root.snapCompleted(record);
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
                        root.activated(record.id, record.first, record.modifiers, record.focus_context || "");
                    } else if (record.event === "key")
                        root.keyPressed(record.key, record.modifiers, "");
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
                        if (root.sessionSerial) {
                            root.activeSessionBindingId = record.id;
                            root.activeSessionId = String(root.sessionSerial);
                        }
                        root.activated(record.id, record.first !== false, record.modifiers || 0, record.focus_context || "");
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.activated") {
                        root.sessionSerial = record.session_id || 0;
                        root.activeSessionBindingId = record.id;
                        root.activeSessionId = String(root.sessionSerial);
                        if (record.first === false)
                            root.activated(record.id, false, record.modifiers || 0, "");
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.key") {
                        if (record.phase === "press")
                            root.keyPressed(record.keyval, record.modifiers || 0, record.focus_context || "");
                        else if (record.phase === "release")
                            root.keyReleased(record.keyval, record.modifiers || 0, record.focus_context || "");
                    } else if (root.nativeProtocol && record.event === "gnoblin.shortcut.session.ended") {
                        const sessionId = String(record.session_id || "");
                        const endedExplicitly = root.endingSessionIds.includes(sessionId);
                        if (endedExplicitly)
                            root.endingSessionIds = root.endingSessionIds.filter(id => id !== sessionId);
                        if (root.activeSessionId === sessionId) {
                            root.activeSessionBindingId = "";
                            root.activeSessionId = "";
                            root.sessionSerial = 0;
                        }
                        if (endedExplicitly) {
                            // The caller already chose whether to finish or cancel.
                        } else if (record.reason === "released")
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
