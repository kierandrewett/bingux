pragma Singleton
import QtQuick
import Quickshell
import "BsmSession.js" as Session
import "BsmReadiness.js" as Protocol

// Whether this Bingux session is hosted by bsm (the Bingux Session Manager).
// bsm-helper sets BSM_SESSION_ID for a hosted session only; a standalone
// Gnoblin and Bingux never sees it, and then every bsm code path stays off.
// BSM_GENERATION names this launch and is sent with every readiness line.
// Reading the environment here keeps wallpaper and popout processes from
// loading the request client in BsmSession.
QtObject {
    id: root
    readonly property string sessionId: Quickshell.env("BSM_SESSION_ID") || ""
    readonly property string generation: Quickshell.env("BSM_GENERATION") || ""
    readonly property bool integrated: Session.integrated(sessionId)
    readonly property bool generationValid: Protocol.validGeneration(generation)
    readonly property string readySocketPath: (Quickshell.env("XDG_RUNTIME_DIR") || "") + "/bsm/ready.sock"
}
