import QtQuick
import Quickshell
import "../auth"
import "../auth/ProgressModel.js" as Progress
import "../auth/Availability.js" as Availability

// Everything the bsm greeter shows, as one value: the last revisioned
// snapshot from bsm, the selected account, the startup progress of the launch
// in flight and the state of the sign-in conversation.
//
// This item decides nothing about the session. It validates what arrives from
// bsm (image paths must lie under the greeter cache, appearance fields are
// whitelisted by AuthTheme, progress models are shape-checked) and holds the
// result. BsmAuthClient feeds it and acts on it; the screens only read it.
//
// A secret is never stored here. The response field hands its text to
// BsmAuthClient once and is cleared in the same step.
QtObject {
    id: root

    // The greeter-readable image cache bsm writes (bsm-proto GREETER_CACHE_DIR).
    readonly property string cacheDir: {
        // Quickshell.env gives null for an unset variable, not "".
        const configured = Quickshell.env("BSM_GREETER_CACHE");
        const dir = typeof configured === "string" && configured !== "" ? configured : "/var/lib/bingux/greeter";
        return dir.endsWith("/") ? dir : dir + "/";
    }

    // Connection to bsm.
    property bool connected: false
    property bool synced: false
    property string fatalError: ""

    // The applied snapshot.
    property real revision: -1
    property string modeKind: "login"
    property int unlockUid: -1
    property string unlockSession: ""
    property int selectedUid: -1
    property var users: []
    property string hostname: ""
    property bool reduceMotion: false

    readonly property bool unlockMode: modeKind === "unlock"
    readonly property var selectedUser: {
        for (let i = 0; i < users.length; i++) {
            if (users[i].uid === selectedUid)
                return users[i];
        }
        return null;
    }
    readonly property var unlockUser: {
        for (let i = 0; i < users.length; i++) {
            if (users[i].uid === unlockUid)
                return users[i];
        }
        return null;
    }
    // bsm answers busy while a lock or switch fade runs, so the picker waits for it.
    readonly property bool canSwitchUser: synced && users.length > 1 && !showProgress && !signingIn && !operationActive

    // Account list for the picker.
    readonly property var pickerUsers: users.map(user => ({
                uid: user.uid,
                userName: user.userName,
                displayName: user.displayName,
                avatar: user.avatar,
                availability: user.availability
            }))

    // Startup progress of the launch in flight (normalised), and whether the
    // user left a failed launch behind.
    property var progress: null
    property string dismissedLaunch: ""
    readonly property bool showProgress: progress !== null && launchKey(progress) !== dismissedLaunch
    readonly property bool launchFailed: progress !== null && progress.failure !== null

    // Sign-in conversation. phase: idle | waiting | prompting | verifying | signing_in
    property string phase: "idle"
    property int authUid: -1
    property real challengeId: 0
    property string promptKind: ""
    property string promptText: ""
    // PAM text that needs no answer (info and error lines) and results.
    property string statusText: ""
    property bool statusIsError: false
    readonly property bool signingIn: phase === "signing_in"
    readonly property bool promptOpen: phase === "prompting" && (promptKind === "secret" || promptKind === "visible")
    readonly property bool busy: phase === "verifying" || phase === "signing_in" || phase === "waiting"

    // Power and other operations.
    property string powerAction: ""
    property real operationId: 0
    property string operationKind: ""
    property string operationPhase: ""
    property string operationReason: ""
    readonly property bool operationActive: operationId !== 0 && ["requested", "authorising", "preparing", "animating", "committed"].includes(operationPhase)
    readonly property bool operationCancellable: operationId !== 0 && powerAction !== "" && ["requested", "authorising", "preparing"].includes(operationPhase)
    readonly property bool powerActive: powerAction !== "" && operationActive

    readonly property bool canAuthenticate: synced && connected && selectedUser !== null && Availability.selectable(selectedUser.availability) && !showProgress && !signingIn && !powerActive
    readonly property bool canSubmit: promptOpen && connected && !powerActive
    readonly property bool canRetry: canAuthenticate && phase === "idle"

    readonly property string intent: {
        if (selectedUser === null)
            return "login";
        if (unlockMode && selectedUid === unlockUid)
            return "unlock";
        return Availability.hasSession(selectedUser.availability) || unlockMode ? "switch" : "login";
    }

    readonly property string hint: {
        if (!connected)
            return "Waiting for the session manager";
        if (!synced)
            return "Starting";
        if (selectedUser === null)
            return users.length === 0 ? "No accounts are available" : "Choose an account";
        if (!Availability.selectable(selectedUser.availability))
            return selectedUser.availability === "starting" ? "This account is starting" : "This account is unavailable";
        return intent === "unlock" ? "Enter your password to unlock" : "Enter your password to sign in";
    }

    readonly property string powerText: {
        const verb = powerAction === "reboot" ? "Restarting" : powerAction === "poweroff" ? "Shutting down" : "Suspending";
        if (operationPhase === "failed")
            return "Could not complete this request" + (operationReason !== "" ? ": " + reasonText(operationReason) : "");
        if (operationPhase === "cancelled")
            return "Cancelled";
        return verb + "…";
    }

    // What the status line says: an operation, a conversation message or the hint.
    readonly property string statusLine: {
        if (fatalError !== "")
            return fatalError;
        if (powerAction !== "" && operationPhase !== "" && operationPhase !== "completed")
            return powerText;
        if (signingIn)
            return "Signing in…";
        if (statusText !== "")
            return statusText;
        return hint;
    }
    readonly property bool statusLineIsError: fatalError !== "" || (powerAction !== "" && operationPhase === "failed") || (!signingIn && statusText !== "" && statusIsError)

    readonly property string promptPlaceholder: {
        if (!promptOpen || promptText === "")
            return "Password";
        const trimmed = promptText.replace(/[\s:]+$/, "");
        return trimmed !== "" ? trimmed : "Password";
    }

    // Image sources of the selected account. An account that opted out of
    // personal images has no paths, so the neutral fallback shows.
    readonly property url wallpaper: selectedUser !== null && selectedUser.wallpaper !== "" ? selectedUser.wallpaper : ""
    readonly property int wallpaperRevision: selectedUser !== null ? selectedUser.wallpaperRev : 0
    readonly property url avatar: selectedUser !== null ? selectedUser.avatar : ""

    // Emitted when sensitive state must be cleared on screen: an account
    // change, a cancelled conversation, a lost connection.
    signal secretsCleared
    signal rejected

    function launchKey(model) {
        return model.session + ":" + String(model.generation);
    }

    function reasonText(code) {
        switch (code) {
        case "unauthorised":
            return "not allowed";
        case "inhibited":
            return "an application is blocking it";
        case "busy":
            return "bsm is busy";
        case "shutdown_rejected":
            return "the system refused";
        case "timeout":
            return "it took too long";
        case "transition_interrupted":
            return "interrupted";
        default:
            return code.replace(/_/g, " ");
        }
    }

    function authErrorText(code, message) {
        if (message !== "")
            return message;
        switch (code) {
        case "timeout":
            return "Authentication timed out";
        case "helper_failure":
            return "The authentication service is unavailable";
        case "unauthorised":
            return "This account cannot sign in here";
        case "busy":
            return "bsm is busy, try again";
        default:
            return "Authentication failed";
        }
    }

    // Validate a path from bsm: absolute, under the greeter cache, no parent
    // segments. Returns a file URL string or "".
    function imageUrl(path) {
        if (typeof path !== "string" || path === "" || !path.startsWith(cacheDir))
            return "";
        if (path.indexOf("/../") >= 0 || path.endsWith("/..") || path.indexOf("\0") >= 0 || path.indexOf("?") >= 0 || path.indexOf("#") >= 0)
            return "";
        return "file://" + path;
    }

    function normaliseUser(raw) {
        if (raw === null || typeof raw !== "object" || typeof raw.uid !== "number" || typeof raw.user_name !== "string")
            return null;
        const personal = raw.show_personal_images !== false;
        const avatarRev = Number(raw.avatar_rev) || 0;
        const avatarPath = personal ? imageUrl(raw.avatar_path) : "";
        return {
            uid: raw.uid,
            userName: raw.user_name,
            displayName: typeof raw.display_name === "string" && raw.display_name !== "" ? raw.display_name : raw.user_name,
            avatar: avatarPath !== "" ? avatarPath + (avatarRev > 0 ? "?rev=" + avatarRev : "") : "",
            wallpaper: personal ? imageUrl(raw.wallpaper_path) : "",
            wallpaperRev: Number(raw.wallpaper_rev) || 0,
            appearance: raw.appearance !== null && typeof raw.appearance === "object" ? raw.appearance : null,
            availability: typeof raw.availability === "string" ? raw.availability : "unavailable",
            session: typeof raw.session === "string" ? raw.session : ""
        };
    }

    // Forget the conversation and anything typed into it.
    function resetConversation() {
        phase = "idle";
        authUid = -1;
        challengeId = 0;
        promptKind = "";
        promptText = "";
        statusText = "";
        statusIsError = false;
        secretsCleared();
    }

    // Apply a snapshot unless it is older than what is already shown.
    // Returns true when applied.
    function applySnapshot(raw) {
        if (raw === null || typeof raw !== "object" || typeof raw.revision !== "number" || !Array.isArray(raw.users))
            return false;
        if (raw.revision <= revision)
            return false;
        const next = [];
        for (let i = 0; i < raw.users.length; i++) {
            const user = normaliseUser(raw.users[i]);
            if (user !== null)
                next.push(user);
        }
        const mode = raw.mode !== null && typeof raw.mode === "object" ? raw.mode : {};
        const kind = mode.mode === "unlock" ? "unlock" : "login";
        const unlockOwner = kind === "unlock" && typeof mode.uid === "number" ? mode.uid : -1;
        const selected = typeof raw.selected_uid === "number" ? raw.selected_uid : -1;

        // bsm selects the account of every begin_auth: a selection that is the one
        // we are authenticating is the confirmation, not a change.
        const accountChanged = (selected !== selectedUid && selected !== authUid) || kind !== modeKind || unlockOwner !== unlockUid;
        revision = raw.revision;
        modeKind = kind;
        unlockUid = unlockOwner;
        unlockSession = kind === "unlock" && typeof mode.session === "string" ? mode.session : "";
        users = next;
        selectedUid = selected;
        hostname = typeof raw.hostname === "string" ? raw.hostname : "";
        reduceMotion = raw.reduce_motion === true;
        synced = true;
        if (accountChanged)
            resetConversation();

        // Progress: take bsm's model unless it is the launch already shown and
        // not newer. A snapshot without progress ends the launch view.
        const incoming = Progress.normalise(raw.progress);
        if (incoming === null) {
            progress = null;
            dismissedLaunch = "";
        } else if (!Progress.sameLaunch(progress, incoming) || incoming.revision > progress.revision) {
            progress = incoming;
        }
        return true;
    }

    // A progress event. It moves the bar only for the launch in flight: one
    // that bsm announced in a snapshot, or the launch that follows our own
    // accepted sign-in. A stale revision or another launch is dropped.
    function applyProgress(raw) {
        const incoming = Progress.normalise(raw);
        if (incoming === null)
            return false;
        if (progress !== null) {
            if (!Progress.sameLaunch(progress, incoming) || incoming.revision <= progress.revision)
                return false;
        } else if (!signingIn) {
            return false;
        }
        progress = incoming;
        return true;
    }

    function dismissLaunch() {
        if (progress !== null)
            dismissedLaunch = launchKey(progress);
    }

    function applyOperation(id, kind, opPhase, reason) {
        // Operations of a power request we sent drive the status line; any
        // other operation only matters when it fails after our sign-in.
        operationId = id;
        operationKind = kind;
        operationPhase = opPhase;
        operationReason = reason || "";
        if (powerAction === "" && opPhase === "failed" && signingIn) {
            phase = "idle";
            statusText = "Signing in did not complete" + (reason ? ": " + reasonText(reason) : "");
            statusIsError = true;
            rejected();
        }
        if (opPhase === "completed")
            powerAction = "";
    }

    function clearOperation() {
        powerAction = "";
        operationId = 0;
        operationKind = "";
        operationPhase = "";
        operationReason = "";
    }

    // The connection went away: drop the conversation and the operation, keep
    // the last appearance on screen so the greeter does not flash.
    function disconnected() {
        connected = false;
        synced = false;
        revision = -1;
        progress = null;
        dismissedLaunch = "";
        clearOperation();
        resetConversation();
    }
}
