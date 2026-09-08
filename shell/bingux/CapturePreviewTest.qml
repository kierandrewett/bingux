import QtQuick
import Quickshell

ShellRoot {
    CaptureTool { id: capture; screen: Quickshell.screens[0] }
    Timer {
        interval: 300; running: true
        onTriggered: {
            capture.open();
            capture.optionsOpen = Quickshell.env("BINGUX_CAPTURE_TEST_OPTIONS") === "1";
        }
    }
    Timer { interval: 3500; running: true; onTriggered: { capture.close(); Qt.quit(); } }
}
