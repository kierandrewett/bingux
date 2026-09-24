//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import Bingux.Wayland

// This is deliberately separate from shell.qml. The desktop shell can reload
// or fail without taking a secure lock client down with it.
ShellRoot {
    id: root
    property bool secureSeen: false

    // A reload destroys the current QML objects. That is appropriate for the
    // desktop shell but unsafe for a session lock, whose compositor state is
    // intentionally retained if the client dies.
    Component.onCompleted: {
        Quickshell.watchFiles = false;
        lock.locked = true;
    }

    LockAuthentication {
        id: lockAuthentication
        sessionLock: lock
        onUnlockSubmitted: unlockRoundtrip.synchronize()
    }

    WaylandRoundtrip {
        id: unlockRoundtrip

        // ext_session_lock_v1 requires a wl_display.sync before a client exits
        // after unlock_and_destroy. This callback is ordered after the unlock
        // request on Qt's own Wayland connection.
        onCompleted: {
            if (lockAuthentication.unlockRequested)
                Qt.quit();
        }
        onFailed: message => {
            if (lockAuthentication.unlockRequested) {
                lockAuthentication.status = "Unlock request sent; waiting for the compositor";
                lockAuthentication.statusIsError = true;
            }
        }
    }

    WlSessionLock {
        id: lock

        onSecureChanged: {
            if (secure)
                root.secureSeen = true;
            else
            // Quickshell 0.2.1 maps ext_session_lock_v1.finished to this
            // transition. A compositor-finished lock can safely exit. PAM
            // unlock_and_destroy is followed by WaylandRoundtrip. A compositor
            // finished lock can safely exit immediately; the PAM path exits
            // only after the ordered wl_display.sync callback.
            if (root.secureSeen && !lockAuthentication.unlockRequested)
                Qt.quit();
        }
        onLockedChanged: if (!locked && !root.secureSeen)
            Qt.quit()

        WlSessionLockSurface {
            color: "#111318"
            LockScreen {
                anchors.fill: parent
                authentication: lockAuthentication
            }
        }
    }
}
