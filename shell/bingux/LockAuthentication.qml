import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pam

// One PAM conversation belongs to the session, not an output. SessionScreen
// is instantiated once per output by WlSessionLock, so putting PamContext
// there would permit concurrent authentication attempts on multi-monitor
// systems.
//
// This implements the SessionScreen authentication contract. A second,
// fingerprint-only conversation runs beside the password one when fprintd
// reports an enrolled finger, so either method unlocks and neither blocks the
// other. The greeter cannot do this: greetd owns a single PAM conversation.
QtObject {
    id: root

    required property var sessionLock
    property bool unlockRequested: false
    readonly property bool finishing: unlockRequested
    property string status: ""
    property bool statusIsError: false
    property string hint: "Enter your password to unlock"
    readonly property bool active: pam.active
    readonly property bool responseRequired: pam.responseRequired
    readonly property bool responseVisible: pam.responseVisible
    readonly property string prompt: pam.message
    property bool fingerprintAvailable: false
    property bool faceAvailable: false
    readonly property string userName: Quickshell.env("USER")
    property string userDisplayName: userName
    property url userImage: ""
    property url wallpaper: ""
    signal responseRequested
    signal unlockSubmitted

    function start() {
        if (!sessionLock.secure || pam.active)
            return;
        status = "";
        statusIsError = false;
        pam.start();
        if (fingerprintAvailable && !fingerprint.active)
            fingerprint.start();
    }

    function respond(value) {
        if (pam.active && pam.responseRequired)
            pam.respond(value);
    }

    function unlock() {
        unlockRequested = true;
        status = "Unlocking…";
        statusIsError = false;
        if (pam.active)
            pam.abort();
        if (fingerprint.active)
            fingerprint.abort();
        sessionLock.locked = false;
        unlockSubmitted();
    }

    function updateHint() {
        if (fingerprintAvailable && faceAvailable)
            hint = "Face, fingerprint or enter your password";
        else if (fingerprintAvailable)
            hint = "Fingerprint or enter your password";
        else if (faceAvailable)
            hint = "Face or enter your password";
        else
            hint = "Enter your password to unlock";
    }

    property Process account: Process {
        command: ["bingux-session", "users"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const match = JSON.parse(text).users.find(user => user.name === root.userName);
                    if (match) {
                        root.userDisplayName = match.displayName || root.userName;
                        root.userImage = match.icon ? "file://" + match.icon : "";
                    }
                } catch (_) {}
            }
        }
    }

    property Process wallpaperReader: Process {
        command: ["bingux-session", "wallpaper"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const path = JSON.parse(text).wallpaper;
                    root.wallpaper = typeof path === "string" && path.startsWith("/") ? "file://" + path : "";
                } catch (_) {
                    root.wallpaper = "";
                }
            }
        }
    }

    property Process biometrics: Process {
        command: ["bingux-session", "biometrics", root.userName, "bingux-lock"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    root.fingerprintAvailable = result.fingerprint === true;
                    root.faceAvailable = result.face === true;
                } catch (_) {
                    root.fingerprintAvailable = false;
                    root.faceAvailable = false;
                }
                root.updateHint();
                if (root.fingerprintAvailable && root.sessionLock.secure && !root.unlockRequested && !fingerprint.active)
                    fingerprint.start();
            }
        }
    }

    property var pam: PamContext {
        config: "bingux-lock"

        onPamMessage: {
            root.status = message || "";
            root.statusIsError = messageIsError;
            if (responseRequired)
                root.responseRequested();
        }
        onCompleted: result => {
            if (root.unlockRequested)
                return;
            if (result === PamResult.Success && root.sessionLock.secure) {
                root.unlock();
                return;
            }
            if (result === PamResult.Success) {
                root.status = "The lock session ended before authentication completed";
                root.statusIsError = true;
                return;
            }
            root.status = result === PamResult.MaxTries ? "Too many authentication attempts" : "Authentication failed";
            root.statusIsError = true;
            root.responseRequested();
        }
        onError: _error => {
            root.status = "Authentication service error";
            root.statusIsError = true;
        }
    }

    // Fingerprint-only stack. Messages from the reader ("Place your finger")
    // are informational and never replace the password prompt.
    property var fingerprint: PamContext {
        config: "bingux-lock-fingerprint"
        onCompleted: result => {
            if (root.unlockRequested)
                return;
            if (result === PamResult.Success && root.sessionLock.secure) {
                root.unlock();
                return;
            }
            // Let the user retry with a finger without touching the password
            // conversation; a persistent failure just leaves the password.
            if (result !== PamResult.MaxTries && root.sessionLock.secure && root.pam.active)
                fingerprint.start();
        }
        onError: _error => {}
    }
}
