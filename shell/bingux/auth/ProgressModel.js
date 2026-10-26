.pragma library

// The startup progress model as bsm sends it (bsm-proto `ProgressModel`), and
// the same arithmetic as the Rust side: the weights, the labels and the
// fraction are the documented work units of packages/bsm-proto/src/progress.rs.
// tests/bsm-greeter-progress.test.py keeps this table equal to the Rust one.
//
// Nothing here reads a clock. The fraction moves only when the model does.

var STAGES = [
    {id: "auth_accepted", weight: 5, counted: false, label: "Authenticating", failure: "The sign-in was not accepted"},
    {id: "pam_session", weight: 10, counted: false, label: "Opening your session", failure: "Your session could not be opened"},
    {id: "host_connected", weight: 5, counted: false, label: "Starting Gnoblin", failure: "Gnoblin could not be started"},
    {id: "gnoblin_ready", weight: 20, counted: false, label: "Starting Gnoblin", failure: "Gnoblin did not become ready"},
    {id: "outputs_configured", weight: 10, counted: true, label: "Preparing displays", failure: "The displays could not be prepared"},
    {id: "bingux_launched", weight: 10, counted: false, label: "Starting Bingux", failure: "Bingux could not be started"},
    {id: "surfaces_mapped", weight: 20, counted: true, label: "Waiting for Bingux surfaces", failure: "The desktop surfaces did not appear"},
    {id: "desktop_ready", weight: 15, counted: true, label: "Waiting for the first frame", failure: "The desktop did not draw its first frame"},
    {id: "ready_to_reveal", weight: 5, counted: false, label: "Ready", failure: "The desktop could not be shown"}
];

function stage(id) {
    for (var i = 0; i < STAGES.length; i++) {
        if (STAGES[i].id === id)
            return STAGES[i];
    }
    return null;
}

// Accept only a well-formed model. Returns null for anything else, so a
// malformed record from the wire can never reach the screen.
function normalise(raw) {
    if (raw === null || typeof raw !== "object")
        return null;
    if (typeof raw.session !== "string" || !Array.isArray(raw.reached))
        return null;
    var reached = [];
    for (var i = 0; i < raw.reached.length; i++) {
        if (stage(raw.reached[i]) === null)
            return null;
        reached.push(raw.reached[i]);
    }
    var failure = null;
    if (raw.failure !== null && raw.failure !== undefined) {
        if (typeof raw.failure !== "object" || stage(raw.failure.stage) === null)
            return null;
        failure = {
            stage: raw.failure.stage,
            code: String(raw.failure.code || ""),
            message: String(raw.failure.message || "")
        };
    }
    var counted = raw.counted_stage === null || raw.counted_stage === undefined ? null : raw.counted_stage;
    if (counted !== null && stage(counted) === null)
        return null;
    return {
        session: raw.session,
        // The generation is only ever compared for equality; JSON numbers
        // beyond 2^53 lose low bits but equal values stay equal.
        generation: raw.generation,
        revision: Number(raw.revision) || 0,
        reached: reached,
        countedStage: counted,
        done: Math.max(0, Number(raw.done) || 0),
        total: Math.max(0, Number(raw.total) || 0),
        failure: failure
    };
}

function has(model, id) {
    return model.reached.indexOf(id) >= 0;
}

// The first stage not yet reached, or null when all are.
function pending(model) {
    for (var i = 0; i < STAGES.length; i++) {
        if (!has(model, STAGES[i].id))
            return STAGES[i];
    }
    return null;
}

// Weighted composite, 0..100. Reached stages count fully; the stage in
// progress counts proportionally only when it is a counted stage with a known
// total. An uncounted pending stage contributes nothing until it is reached.
function fraction(model) {
    var sum = 0;
    for (var i = 0; i < model.reached.length; i++)
        sum += stage(model.reached[i]).weight;
    var next = pending(model);
    if (next !== null && model.countedStage === next.id && model.total > 0)
        sum += Math.floor(next.weight * Math.min(model.done, model.total) / model.total);
    return sum;
}

// True while the pending stage has no counts: the bar holds still and the
// screen shows an activity treatment instead of a guess.
function indeterminate(model) {
    return model.failure === null && pending(model) !== null && model.total === 0;
}

function complete(model) {
    return pending(model) === null && model.failure === null;
}

function label(model) {
    var next = pending(model);
    return next === null ? "Ready" : next.label;
}

// "1 of 2" while a counted stage reports its counts, else an empty string.
function counts(model) {
    var next = pending(model);
    if (model.failure === null && next !== null && model.countedStage === next.id && model.total > 0)
        return model.done + " of " + model.total;
    return "";
}

function failureTitle(model) {
    return model.failure === null ? "" : stage(model.failure.stage).failure;
}

// Same session and launch.
function sameLaunch(a, b) {
    return a !== null && b !== null && a.session === b.session && String(a.generation) === String(b.generation);
}
