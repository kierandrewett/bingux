//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland

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
    }

    WlSessionLock {
        id: lock

        onSecureChanged: {
            if (secure)
                root.secureSeen = true;
            else
            // Quickshell 0.2.1 maps ext_session_lock_v1.finished to this
            // transition. A compositor-finished lock can safely exit. PAM
            // unlock sends unlock_and_destroy asynchronously, so that path
            // deliberately remains alive; QML has no wl_display.sync API.
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
