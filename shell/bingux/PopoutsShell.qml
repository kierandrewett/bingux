//@ pragma UseQApplication
import QtQuick
import Quickshell

// Search, the window switcher, capture and the emoji picker share one process.
// Four separate Quickshell processes each paid for Qt start-up, a GL context and
// their own copy of the shared singletons while the bar and dock were still
// loading. The popouts wait for the desktop shell to map its dock so they never
// compete with it, and still start on their own if the shell never reports in.
ShellRoot {
    id: root
    property bool desktopReady: false
    Connections {
        target: Quickshell
        function onReloadCompleted() {
            Quickshell.inhibitReloadPopup();
        }
    }
    // Read the desktop shell's state without owning a popout.
    UiSession {
        sessionName: "popouts"
        onStatesChanged: if (states.desktop?.ready === true)
            root.desktopReady = true
    }
    Timer {
        interval: 8000
        running: true
        onTriggered: root.desktopReady = true
    }
    LazyLoader {
        active: root.desktopReady
        SearchHost {}
    }
    LazyLoader {
        active: root.desktopReady
        SwitcherHost {}
    }
    LazyLoader {
        active: root.desktopReady
        CaptureHost {}
    }
    LazyLoader {
        active: root.desktopReady
        EmojiHost {}
    }
}
