import Quickshell
import Quickshell.Io

ShellRoot {
    CaptureTool { id: capture; screen: Quickshell.screens[0] }
    IpcHandler {
        target: "test"
        function reload(): void { Qt.callLater(() => Quickshell.reload(false)); }
        function region(): string { return JSON.stringify(capture.region); }
        function setRegion(): void { capture.region = Qt.rect(100, 100, 240, 160); capture.hasRegion = true; }
    }
}
