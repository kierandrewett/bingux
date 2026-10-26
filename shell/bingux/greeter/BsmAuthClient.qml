import QtQuick
import Quickshell
import Quickshell.Io

// The greeter's side of the bsm control socket: newline-delimited JSON as
// defined in packages/bsm-proto/src/control.rs.
//
// On connect it sends `hello`; bsm answers with a full snapshot and then
// events (prompts, results, progress, operations, errors). Every message
// that could belong to an earlier account, request or launch is dropped:
//   - snapshots older than the one applied;
//   - prompts and results whose request or challenge is not the current one;
//   - progress that is not the launch in flight.
// A secret leaves this item only inside `auth_response`, is never stored and
// never logged. Log lines name message types, never contents.
//
// The conversation follows the account bsm says is selected: when the
// selection changes, the old conversation is cancelled, secrets are cleared
// and a new one begins for the new account. Selecting an account only asks
// bsm to change the appearance; it never reveals or starts a session.
QtObject {
    id: root

    required property var model
    property string path: ""

    readonly property int protocolVersion: 1
    property real lastRequest: 0
    // Request ids of the messages we are waiting on.
    property real authRequest: 0
    property real powerRequest: 0
    property real selectRequest: 0
    // Responses submitted since the conversation began; decides whether a
    // failure restarts the conversation by itself.
    property int responses: 0
    property int unansweredFailures: 0

    function start() {
        link.connected = true;
    }

    function nextRequest() {
        lastRequest += 1;
        return lastRequest;
    }

    // Every message the client composes, whether or not a socket is up. Tests
    // listen here; nothing in the shell does.
    signal sent(var message)

    function send(message) {
        sent(message);
        if (!link.connected)
            return false;
        link.write(JSON.stringify(message) + "\n");
        link.flush();
        return true;
    }

    // Ask bsm to show another account. The conversation for it begins when
    // bsm's snapshot confirms the selection.
    function selectUser(uid) {
        if (uid === root.model.selectedUid) {
            reconcile();
            return;
        }
        cancelConversation();
        root.model.resetConversation();
        selectRequest = nextRequest();
        send({
            type: "select_user",
            uid: uid
        });
    }

    function cancelConversation() {
        if (root.model.challengeId !== 0)
            send({
                type: "cancel_auth",
                challenge_id: root.model.challengeId
            });
        authRequest = 0;
    }

    function beginAuth() {
        const model = root.model;
        if (!model.canAuthenticate || model.phase !== "idle")
            return;
        cancelConversation();
        responses = 0;
        authRequest = nextRequest();
        model.authUid = model.selectedUid;
        model.challengeId = 0;
        model.phase = "waiting";
        send({
            type: "begin_auth",
            request_id: authRequest,
            uid: model.selectedUid,
            intent: model.intent
        });
    }

    // Start a conversation when the screen should have one and has none, and
    // end one that no longer matches the selected account.
    function reconcile() {
        const model = root.model;
        if (model.phase !== "idle" && model.authUid !== model.selectedUid) {
            cancelConversation();
            model.resetConversation();
        }
        if (model.canAuthenticate && model.phase === "idle")
            beginAuth();
    }

    // Hand a typed answer to bsm once. The caller has already cleared its field.
    function respond(text) {
        const model = root.model;
        if (!model.promptOpen || model.challengeId === 0)
            return;
        const challenge = model.challengeId;
        model.phase = "verifying";
        model.promptKind = "";
        model.promptText = "";
        model.statusText = "";
        model.statusIsError = false;
        responses += 1;
        send({
            type: "auth_response",
            challenge_id: challenge,
            response: text
        });
    }

    function retry() {
        root.model.dismissLaunch();
        root.model.resetConversation();
        reconcile();
    }

    // Leaving a failed launch tells bsm, which keeps the failed progress in its
    // snapshots until it is cancelled (any operation id will do) or retried.
    function backToAccounts() {
        if (root.model.launchFailed)
            send({
                type: "cancel_operation",
                operation_id: root.model.operationId
            });
        root.model.dismissLaunch();
        root.model.resetConversation();
        reconcile();
    }

    function power(action) {
        root.model.clearOperation();
        root.model.powerAction = action;
        root.model.operationPhase = "requested";
        powerRequest = nextRequest();
        send({
            type: "power",
            request_id: powerRequest,
            action: action
        });
    }

    function cancelOperation() {
        if (root.model.operationId !== 0)
            send({
                type: "cancel_operation",
                operation_id: root.model.operationId
            });
    }

    // One decoded line from bsm.
    function handle(message) {
        const model = root.model;
        switch (message.type) {
        case "snapshot":
            {
                // The snapshot is the message itself: serde flattens the
                // struct next to the `type` tag.
                const challenge = model.challengeId;
                if (model.applySnapshot(message)) {
                    // An account or mode change ended the conversation locally;
                    // tell bsm too, then follow the new selection.
                    if (challenge !== 0 && model.challengeId === 0 && model.phase === "idle")
                        send({
                            type: "cancel_auth",
                            challenge_id: challenge
                        });
                    reconcile();
                }
            }
            break;
        case "auth_prompt":
            handlePrompt(message);
            break;
        case "auth_result":
            handleResult(message);
            break;
        case "progress":
            model.applyProgress(message);
            break;
        case "operation":
            if (typeof message.operation_id === "number")
                model.applyOperation(message.operation_id, String(message.kind), String(message.phase), message.reason || "");
            if (!model.powerActive && !model.signingIn)
                reconcile();
            break;
        case "error":
            handleError(message);
            break;
        default:
            console.warn("[bsm-greeter] ignoring a message of an unknown type");
        }
    }

    function handlePrompt(message) {
        const model = root.model;
        if (typeof message.challenge_id !== "number" || typeof message.text !== "string")
            return;
        if (message.request_id !== null && message.request_id !== undefined) {
            // The first prompt of a conversation answers our begin_auth.
            if (message.request_id !== authRequest)
                return;
            if (model.challengeId === 0)
                model.challengeId = message.challenge_id;
        }
        if (message.challenge_id !== model.challengeId || model.phase === "signing_in")
            return;
        switch (message.kind) {
        case "secret":
        case "visible":
            model.promptKind = message.kind;
            model.promptText = message.text;
            model.phase = "prompting";
            break;
        case "info":
        case "error":
            model.statusText = message.text;
            model.statusIsError = message.kind === "error";
            break;
        default:
            return;
        }
    }

    function handleResult(message) {
        const model = root.model;
        if (typeof message.challenge_id !== "number" || message.challenge_id !== model.challengeId || model.challengeId === 0)
            return;
        model.challengeId = 0;
        model.promptKind = "";
        model.promptText = "";
        if (message.ok === true) {
            unansweredFailures = 0;
            model.phase = "signing_in";
            model.statusText = "";
            model.statusIsError = false;
            return;
        }
        if (message.code === "cancelled") {
            model.phase = "idle";
            return;
        }
        model.phase = "idle";
        model.statusText = model.authErrorText(message.code || "", typeof message.message === "string" ? message.message : "");
        model.statusIsError = true;
        model.rejected();
        // A rejected password starts the next attempt at once. A conversation
        // that failed before any answer was given (a locked account, a broken
        // service) waits for the user instead of looping.
        if (responses > 0) {
            unansweredFailures = 0;
            beginAuth();
        } else {
            unansweredFailures += 1;
        }
    }

    function handleError(message) {
        const model = root.model;
        const id = message.request_id;
        const code = String(message.code || "");
        if (code === "stale_generation" || code === "cancelled")
            return;
        if (code === "not_supported") {
            model.fatalError = "This greeter does not match the session manager";
            return;
        }
        if (id === authRequest && authRequest !== 0) {
            authRequest = 0;
            model.phase = "idle";
            model.challengeId = 0;
            model.statusText = model.authErrorText(code, typeof message.message === "string" ? message.message : "");
            model.statusIsError = true;
            model.rejected();
            return;
        }
        if (id === powerRequest && powerRequest !== 0) {
            powerRequest = 0;
            model.operationPhase = "failed";
            model.operationReason = code;
            return;
        }
        if (id === selectRequest && selectRequest !== 0) {
            selectRequest = 0;
            model.statusText = "That account is not available";
            model.statusIsError = true;
            return;
        }
    }

    function parse(line) {
        if (line.length === 0 || line.length > 65536) {
            console.warn("[bsm-greeter] dropped a message of unexpected size");
            return;
        }
        let message = null;
        try {
            message = JSON.parse(line);
        } catch (_) {
            console.warn("[bsm-greeter] dropped a malformed message");
            return;
        }
        if (message === null || typeof message !== "object" || typeof message.type !== "string") {
            console.warn("[bsm-greeter] dropped a message without a type");
            return;
        }
        handle(message);
    }

    property Socket link: Socket {
        path: root.path
        parser: SplitParser {
            splitMarker: "\n"
            onRead: line => root.parse(line)
        }
        onConnectedChanged: {
            if (connected) {
                reconnect.stop();
                root.model.connected = true;
                root.model.fatalError = "";
                root.send({
                    type: "hello",
                    version: root.protocolVersion
                });
            } else {
                root.model.disconnected();
                root.authRequest = 0;
                root.powerRequest = 0;
                root.selectRequest = 0;
                reconnect.restart();
            }
        }
        onError: _error => {
            // The connection never came up: keep trying while bsm starts.
            if (!connected)
                reconnect.restart();
        }
    }

    property Timer reconnect: Timer {
        interval: 1000
        onTriggered: {
            link.connected = false;
            link.connected = true;
        }
    }
}
