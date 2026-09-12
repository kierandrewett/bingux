import Bingux.Settings
import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io

ShellRoot {
    BinguxSettings {
        id: settings
        visible: true
        currentLayout: BinguxPreferences.data.desktop.layout
        currentSidebarEdge: BinguxPreferences.data.desktop.sidebarEdge || "right"
    }
    WindowFrame {
        window: settings
        margin: settings.shadowMargin
    }
    IpcHandler {
        target: "settings"
        function open(): void {
            if (settings.visibility === Window.Minimized)
                settings.showNormal();
            settings.visible = true;
            settings.raise();
            settings.requestActivate();
        }
        function maximise(): void {
            settings.toggleMaximised();
        }
        function close(): void {
            settings.visible = false;
        }
        function toggle(): void {
            settings.visible = !settings.visible;
        }
        function status(): string {
            return JSON.stringify({
                open: settings.visible,
                ready: settings.ready,
                busy: settings.busy,
                status: settings.status
            });
        }
    }
}
