pragma Singleton
import QtQuick
import Quickshell

// The compositor owns the lock request. This object only confirms the two
// protocol boundaries that it can observe: every output is covered, and a
// successfully authenticated session has stopped being locked.
QtObject {
    id: root

    readonly property string token: Quickshell.env("GNOBLIN_LOCK_TOKEN")
    readonly property bool available: token.length > 0

    function reportPresented() {
        if (!available) {
            console.error("bingux-lock: refusing to report presentation without GNOBLIN_LOCK_TOKEN");
            return false;
        }
        // Keep the token out of command-line arguments. The helper obtains it
        // from its inherited environment and makes the typed D-Bus call.
        Quickshell.execDetached(["python3", decodeURIComponent(Qt.resolvedUrl("lock-broker.py").toString().replace(/^file:\/\//, "")), "presented"]);
        return true;
    }
}
