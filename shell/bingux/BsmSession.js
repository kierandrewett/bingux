// Pure logic behind BsmSession.qml: the bsmctl command line, the answer to a
// request, the monitor event stream and the tracking of one operation. No QML
// object is used here, so tests/bsm-session.test.mjs runs it under Node.
//
// Contract with bsmctl (packages/bsm, docs/bsm/shell-integration.md):
//   bsmctl ACTION --json   stdout: one JSON line {"operation_id": N} and exit 0
//                          when bsm accepted the request; on refusal exit != 0
//                          and one JSON line {"error": CODE, "message": TEXT}
//   bsmctl monitor --json  stdout: one bsm_proto::MonitorEvent per line
// An accepted request is NOT a success. The outcome is only what the monitor
// reports for the operation id.

var ACTIONS = ["lock", "logout", "switch-user", "suspend", "reboot", "poweroff"];
var PHASES = ["requested", "authorising", "preparing", "animating", "committed", "failed", "cancelled", "completed"];
var FINAL_PHASES = ["failed", "cancelled", "completed"];
var MONITOR_EVENTS = ["sessions_changed", "foreground_changed", "lock_state_changed", "transition", "operation_finished"];
// Operations kept for ids that arrive before the request answer does.
var KEPT_OPERATIONS = 32;

var MESSAGES = {
    unauthorised: "You are not allowed to do this.",
    invalid_account: "That account cannot be used.",
    no_session: "There is no session to act on.",
    already_locked: "The session is already locked.",
    busy: "The session manager is busy. Try again in a moment.",
    stale_generation: "The session changed while the request was running.",
    cancelled: "The request was cancelled.",
    pam_failure: "Authentication failed.",
    logind_failure: "The system refused the request.",
    output_not_ready: "The displays are not ready yet.",
    transition_interrupted: "The request was interrupted by another change.",
    inhibited: "Another application is blocking this. Close it and try again.",
    shutdown_rejected: "The system did not accept the request.",
    timeout: "The session manager did not answer in time.",
    helper_failure: "A system helper failed.",
    invalid_argument: "The request was not valid.",
    not_supported: "This is not supported on this system.",
    internal: "The session manager hit an internal error.",
    unavailable: "The session manager is not available.",
    monitor_lost: "Lost contact with the session manager. The request may still complete.",
    no_operation: "The session manager accepted the request but did not name an operation."
};

var TITLES = {
    lock: "Could not lock the session",
    logout: "Could not log out",
    "switch-user": "Could not switch user",
    suspend: "Could not suspend",
    reboot: "Could not restart",
    poweroff: "Could not shut down"
};

function integrated(sessionId) {
    return typeof sessionId === "string" && sessionId.length > 0;
}

// argv for an action, or null for a name bsmctl does not take. The shell never
// builds a command line from anything but this list.
function requestCommand(action) {
    return ACTIONS.indexOf(action) < 0 ? null : ["bsmctl", action, "--json"];
}

function monitorCommand() {
    return ["bsmctl", "monitor", "--json"];
}

function messageFor(code) {
    return MESSAGES[code] || "The session manager reported an error.";
}

function titleFor(action) {
    return TITLES[action] || "The session request failed";
}

// The text shown for a failed operation: sentence first, reason code after it.
function describeFailure(code, detail) {
    var sentence = messageFor(code);
    if (detail && !MESSAGES[code])
        sentence = detail;
    return sentence + (code ? " (" + code + ")" : "");
}

function lastJsonLine(text) {
    var lines = String(text || "").split("\n");
    for (var i = lines.length - 1; i >= 0; i--) {
        var line = lines[i].trim();
        if (line === "")
            continue;
        try {
            var value = JSON.parse(line);
            return value !== null && typeof value === "object" ? value : null;
        } catch (e) {
            return null;
        }
    }
    return null;
}

function operationKey(value) {
    if (typeof value === "number" && isFinite(value) && value >= 0 && Math.floor(value) === value)
        return String(value);
    if (typeof value === "string" && /^[0-9]{1,20}$/.test(value))
        return value;
    return null;
}

// Result of `bsmctl ACTION --json`. Exit status, stdout and stderr in; either
// {ok: true, id} or {ok: false, code, detail}.
function parseRequest(exitCode, stdout, stderr) {
    var value = lastJsonLine(stdout);
    if (exitCode === 0) {
        var id = value ? operationKey(value.operation_id) : null;
        if (id !== null)
            return {
                ok: true,
                id: id
            };
        return {
            ok: false,
            code: "no_operation",
            detail: ""
        };
    }
    var code = "";
    var detail = "";
    if (value) {
        var error = value.error;
        if (error !== null && typeof error === "object") {
            code = typeof error.code === "string" ? error.code : "";
            detail = typeof error.message === "string" ? error.message : "";
        } else if (typeof error === "string") {
            code = error;
        }
        if (!code && typeof value.code === "string")
            code = value.code;
        if (!detail && typeof value.message === "string")
            detail = value.message;
    }
    if (!code)
        code = "unavailable";
    if (!detail)
        detail = String(stderr || "").split("\n")[0].trim();
    return {
        ok: false,
        code: code,
        detail: detail
    };
}

// One line of `bsmctl monitor --json`, or null when it is not a known event.
function parseMonitorEvent(line) {
    var value;
    try {
        value = JSON.parse(line);
    } catch (e) {
        return null;
    }
    if (value === null || typeof value !== "object" || MONITOR_EVENTS.indexOf(value.event) < 0)
        return null;
    if (value.event === "transition") {
        var id = operationKey(value.operation_id);
        if (id === null || PHASES.indexOf(value.phase) < 0)
            return null;
        return {
            event: "transition",
            id: id,
            phase: value.phase,
            kind: typeof value.kind === "string" ? value.kind : "",
            reason: typeof value.reason === "string" ? value.reason : ""
        };
    }
    if (value.event === "operation_finished") {
        var finishedId = operationKey(value.operation_id);
        if (finishedId === null || typeof value.success !== "boolean")
            return null;
        return {
            event: "operation_finished",
            id: finishedId,
            success: value.success,
            reason: typeof value.reason === "string" ? value.reason : ""
        };
    }
    return {
        event: value.event
    };
}

// Tracker state. `seen` remembers what the monitor said about operations by
// id, so an event that beats the request answer is not lost. `current` is the
// operation this shell asked for.
//   status: requesting | pending | completed | failed | cancelled
function newTracker() {
    return {
        seen: {},
        order: [],
        current: null
    };
}

function copyCurrent(current) {
    return current === null ? null : {
        action: current.action,
        id: current.id,
        status: current.status,
        phase: current.phase,
        code: current.code,
        detail: current.detail
    };
}

function withCurrent(tracker, current) {
    return {
        seen: tracker.seen,
        order: tracker.order,
        current: current
    };
}

function isActive(tracker) {
    var current = tracker.current;
    return current !== null && (current.status === "requesting" || current.status === "pending");
}

function begin(tracker, action) {
    if (isActive(tracker))
        return tracker;
    return withCurrent(tracker, {
        action: action,
        id: null,
        status: "requesting",
        phase: "",
        code: "",
        detail: ""
    });
}

function remember(tracker, id, patch) {
    var known = tracker.seen[id];
    if (!known) {
        known = {
            phase: "",
            finished: false,
            success: false,
            reason: ""
        };
        tracker.seen[id] = known;
        tracker.order.push(id);
        while (tracker.order.length > KEPT_OPERATIONS)
            delete tracker.seen[tracker.order.shift()];
    }
    // A final outcome is never overwritten by a late or repeated event. Only a
    // reason it lacked may still arrive.
    if (known.finished) {
        if (!known.reason && patch.reason)
            known.reason = patch.reason;
        return known;
    }
    for (var key in patch)
        known[key] = patch[key];
    return known;
}

function applyKnown(current, known) {
    if (known.phase)
        current.phase = known.phase;
    if (!known.finished)
        return;
    if (known.phase === "cancelled" || (!known.success && known.reason === "cancelled")) {
        current.status = "cancelled";
        current.code = known.reason || "cancelled";
    } else if (known.success) {
        current.status = "completed";
    } else {
        current.status = "failed";
        current.code = known.reason || "internal";
    }
}

// The monitor delivered an event. Returns the new tracker.
function observe(tracker, event) {
    if (event === null || (event.event !== "transition" && event.event !== "operation_finished"))
        return tracker;
    var known;
    if (event.event === "transition") {
        var finished = FINAL_PHASES.indexOf(event.phase) >= 0;
        known = remember(tracker, event.id, {
            phase: event.phase,
            finished: finished,
            success: event.phase === "completed",
            reason: event.reason
        });
    } else {
        known = remember(tracker, event.id, {
            finished: true,
            success: event.success,
            reason: event.reason
        });
    }
    var current = tracker.current;
    if (current === null || current.id !== event.id || (current.status !== "pending" && current.status !== "requesting"))
        return tracker;
    var next = copyCurrent(current);
    applyKnown(next, known);
    return withCurrent(tracker, next);
}

// `bsmctl ACTION` answered. Success only means the operation has an id and is
// now being followed; anything the monitor already said about it is applied.
function answered(tracker, result) {
    var current = tracker.current;
    if (current === null || current.status !== "requesting")
        return tracker;
    var next = copyCurrent(current);
    if (!result.ok) {
        next.status = "failed";
        next.code = result.code;
        next.detail = result.detail || "";
        return withCurrent(tracker, next);
    }
    next.id = result.id;
    next.status = "pending";
    if (tracker.seen[result.id])
        applyKnown(next, tracker.seen[result.id]);
    return withCurrent(tracker, next);
}

// The monitor stream ended while an operation was still open.
function monitorLost(tracker) {
    if (!isActive(tracker))
        return tracker;
    var next = copyCurrent(tracker.current);
    next.status = "failed";
    next.code = "monitor_lost";
    return withCurrent(tracker, next);
}

function clearFinished(tracker) {
    return isActive(tracker) ? tracker : withCurrent(tracker, null);
}

// Text for the phase of an open operation. Wording only; no timing.
function phaseLabel(phase) {
    switch (phase) {
    case "authorising":
        return "Checking permission";
    case "preparing":
        return "Preparing";
    case "animating":
    case "committed":
        return "Working";
    default:
        return "Waiting for the session manager";
    }
}
