//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import Bingux.Wayland
import "../shell/bingux"

// Drives SessionScreen inside a real ext-session-lock surface, with a stand-in
// for LockAuthentication that follows the same contract. The lock must become
// secure, a wrong password must keep the prompt open, and the right one must
// run the exit before finish() sends unlock_and_destroy. Prints
// LOCK_UI_PASSED once the unlock round trip completes.
ShellRoot {
    id: root
    property int step: 0
    property real unlockAt: 0

    function fail(message) {
        console.error("LOCK_UI_FAILED: " + message);
        Qt.exit(1);
    }

    QtObject {
        id: lockAuthentication
        property var sessionLock: lock
        property bool unlockRequested: false
        readonly property bool finishing: unlockRequested
        property bool finished: false
        property bool active: false
        property bool responseRequired: false
        property bool responseVisible: false
        property string prompt: ""
        property string status: ""
        property bool statusIsError: false
        property string hint: "Enter your password to unlock"
        signal responseRequested
        signal unlockSubmitted
        function start() {
            if (!sessionLock.secure)
                root.fail("start() before the lock was secure");
            active = true;
            responseRequired = true;
            prompt = "Password";
            responseRequested();
        }
        function respond(value) {
            responseRequired = false;
            if (value === "secret") {
                unlockRequested = true;
                root.unlockAt = Date.now();
                status = "Unlocking…";
                statusIsError = false;
                return;
            }
            status = "Authentication failed";
            statusIsError = true;
            responseRequired = true;
            responseRequested();
        }
        function finish() {
            if (!unlockRequested || finished)
                return;
            finished = true;
            if (Date.now() - root.unlockAt < Theme.sessionExitMotion)
                root.fail("finish() ran before the exit motion ended");
            sessionLock.locked = false;
            unlockSubmitted();
        }
    }

    WaylandRoundtrip {
        id: unlockRoundtrip
        onCompleted: {
            if (!lockAuthentication.finished)
                root.fail("round trip before finish()");
            if (lock.locked)
                root.fail("still locked after finish()");
            console.error("LOCK_UI_PASSED");
            Qt.quit();
        }
        onFailed: message => root.fail("round trip failed: " + message)
    }
    Connections {
        target: lockAuthentication
        function onUnlockSubmitted() {
            unlockRoundtrip.synchronize();
        }
        function onResponseRequested() {
            if (root.step === 0) {
                root.step = 1;
                lockAuthentication.respond("wrong");
            } else if (root.step === 1) {
                if (!lockAuthentication.statusIsError)
                    root.fail("the wrong password was not reported");
                root.step = 2;
                shot.start();
            }
        }
    }

    // Give the harness a moment to screenshot the locked state, then unlock.
    Timer {
        id: shot
        interval: 1500
        onTriggered: lockAuthentication.respond("secret")
    }
    Timer {
        interval: 15000
        running: true
        onTriggered: root.fail("timed out at step " + root.step + " secure=" + lock.secure)
    }

    Component.onCompleted: lock.locked = true

    WlSessionLock {
        id: lock
        WlSessionLockSurface {
            color: Theme.background
            SessionScreen {
                id: screen
                anchors.fill: parent
                authentication: lockAuthentication
                ready: lock.secure
                accountName: "luna"
                accountDisplayName: "Luna Lovegood"
                wallpaper: Quickshell.env("BINGUX_LOCK_WALLPAPER") !== "" ? "file://" + Quickshell.env("BINGUX_LOCK_WALLPAPER") : ""
            }
        }
    }
}
