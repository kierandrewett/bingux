import QtQuick
import Quickshell.Services.Pam

// One PAM conversation belongs to the session, not an output. LockScreen is
// instantiated once per output by WlSessionLock, so putting PamContext there
// would permit concurrent authentication attempts on multi-monitor systems.
QtObject {
    id: root

    required property var sessionLock
    property bool unlockRequested: false
    property string status: "Enter your password to unlock"
    property bool statusIsError: false
    readonly property bool active: pam.active
    readonly property bool responseRequired: pam.responseRequired
    readonly property bool responseVisible: pam.responseVisible
    readonly property string prompt: pam.message
    signal responseRequested
    signal unlockSubmitted

    function start() {
        if (sessionLock.secure && !pam.active)
            pam.start();
    }

    function respond(value) {
        if (pam.active && pam.responseRequired)
            pam.respond(value);
    }

    property var pam: PamContext {
        config: "bingux-lock"

        onPamMessage: {
            root.status = message || (responseRequired ? "Authentication response required" : "");
            root.statusIsError = messageIsError;
            if (responseRequired)
                root.responseRequested();
        }
        onCompleted: result => {
            if (result === PamResult.Success && root.sessionLock.secure) {
                root.unlockRequested = true;
                root.status = "Unlocking…";
                root.statusIsError = false;
                root.sessionLock.locked = false;
                root.unlockSubmitted();
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
}
