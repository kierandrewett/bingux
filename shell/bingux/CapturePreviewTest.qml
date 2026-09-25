import QtQuick
import Quickshell

ShellRoot {
    UiSession {
        sessionName: "capture-preview"
        state: ({
                visible: capture.opened,
                surface: "bingux-capture",
                companions: ["bingux-capture-controls"],
                companionsAbove: true
            })
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
    }
    Timer {
        interval: 300
        running: true
        onTriggered: {
            if (Quickshell.env("BINGUX_CAPTURE_TEST_KIND"))
                capture.configureOptions(JSON.stringify({
                    kind: Quickshell.env("BINGUX_CAPTURE_TEST_KIND")
                }));
            capture.open();
            capture.optionsOpen = Quickshell.env("BINGUX_CAPTURE_TEST_OPTIONS") === "1";
        }
    }
    Timer {
        interval: 3500
        running: true
        onTriggered: {
            capture.close();
            Qt.quit();
        }
    }
}
