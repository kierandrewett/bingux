//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland

// This is deliberately separate from shell.qml. The desktop shell can reload
// or fail without taking a secure lock client down with it.
ShellRoot {
    id: root

    // A reload destroys the current QML objects. That is appropriate for the
    // desktop shell but unsafe for a session lock, whose compositor state is
    // intentionally retained if the client dies.
    Component.onCompleted: {
        Quickshell.watchFiles = false;
        // A token is issued only by Gnoblin lockd. Refuse standalone locking:
        // without the broker, GNOME/Gnoblin cannot safely coordinate suspend,
        // resume, recovery, or system state.
        if (!LockBroker.available) {
            console.error("bingux-lock: GNOBLIN_LOCK_TOKEN is required");
            Qt.quit();
            return;
        }
        lock.locked = true;
    }

    LockAuthentication {
        id: lockAuthentication
        sessionLock: lock
    }

    WlSessionLock {
        id: lock

        onSecureChanged: {
            if (secure) {
                LockBroker.reportPresented();
            }
        }

        WlSessionLockSurface {
            color: "#111318"
            LockScreen {
                anchors.fill: parent
                authentication: lockAuthentication
            }
        }
    }
}
