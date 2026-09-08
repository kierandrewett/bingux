import QtQuick
import Quickshell
import Quickshell.Wayland

// Exercises real delegate lookup and closing without launching an application.
SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-search-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    property var sample: ({ resultId: "test", providerId: "applications", kind: "application", title: "Launch animation test", subtitle: "No application is launched", icon: "system-file-manager" })
    function check(condition, message) {
        if (!condition) { console.error("FAIL: " + message); Qt.exit(1); }
    }
    Timer {
        interval: 200; running: true
        onTriggered: { preview.displayedResults = [preview.sample]; preview.selectedIndex = 0; }
    }
    Timer {
        interval: 350; running: true
        onTriggered: { preview.animateActivation(preview.sample); preview.closeSearch(); }
    }
    Timer {
        interval: 480; running: true
        onTriggered: preview.check(Theme.reducedMotion ? !preview.visible : preview.visible && preview.closing, "icon copy survives the card closing")
    }
    Timer {
        interval: 750; running: true
        onTriggered: { preview.check(!preview.visible && !preview.closing, "window closes after icon finishes"); Qt.quit(); }
    }
}
