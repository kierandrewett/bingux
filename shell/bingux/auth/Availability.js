.pragma library

// What the account picker says about an account's session. The values are
// bsm-proto `SessionAvailability`. The label always comes from bsm's report
// of the real session: nothing is inferred from a cached profile, and an
// unknown value reads as Unavailable.

function label(availability) {
    switch (availability) {
    case "not_signed_in":
        return "Not signed in";
    case "starting":
        return "Starting";
    case "running_locked":
        return "Running, locked";
    case "running_foreground":
        return "Running";
    case "unavailable":
    default:
        return "Unavailable";
    }
}

// Whether the account can be chosen to sign in or resume.
function selectable(availability) {
    return availability === "not_signed_in" || availability === "running_locked" || availability === "running_foreground";
}

// A session that exists, so choosing the account resumes it rather than
// starting a new one.
function hasSession(availability) {
    return availability === "running_locked" || availability === "running_foreground" || availability === "starting";
}
