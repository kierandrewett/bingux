import QtQuick
import Quickshell
import Quickshell.Wayland

// Native visual regression: captures the actual result icon for alpha inspection.
PanelWindow {
    id: preview
    visible: true
    implicitWidth: 420
    implicitHeight: 210
    anchors { top: true; left: true }
    margins { top: 100; left: 100 }
    color: "#383c44"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    property bool captured: false
    property int attempts: 0
    SearchResult {
        id: row
        width: 420; selected: true; query: "web"
        result: ({ resultId: "test", providerId: "applications", kind: "application", title: "Web", subtitle: "Browse the web", icon: "org.gnome.Epiphany" })
    }
    OsIconImage { id: enlarged; x: 146; y: 65; width: 128; height: 128; source: row.activationIconSource }
    Timer {
        interval: 100; running: true; repeat: true
        onTriggered: {
            preview.attempts++;
            if (enlarged.resolvedSource.startsWith("data:image/png;") && preview.attempts >= 5) {
                stop();
                row.activationIcon.grabToImage(result => {
                    preview.captured = result.saveToFile("/tmp/bingux-search-web-fixed.png");
                });
            } else if (preview.attempts >= 30) {
                console.error("FAIL: SVG was not rendered in memory");
                Qt.exit(1);
            }
        }
    }
    Timer {
        interval: 2000; running: true
        onTriggered: {
            if (!preview.captured) { console.error("FAIL: native icon capture failed"); Qt.exit(1); }
            else { console.warn("PASS: rendered and captured the OS icon through the real search row"); Qt.quit(); }
        }
    }
}
