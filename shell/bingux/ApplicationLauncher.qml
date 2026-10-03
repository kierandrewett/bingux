import QtQuick
import Quickshell
import Quickshell.Wayland

// Launching an application must not wait for its long-lived process to exit.
// Wait for its window and dispatch any follow-up action. The compositor uses
// the application's activation request to decide whether that window gets focus.
Item {
    id: root
    required property string desktopId
    property var windows: ToplevelManager.toplevels.values
    property var nativeWindows: []
    property var pendingCommand: []
    property int attempts: 0
    signal launchStarted(string desktopId)
    signal failed(string message)
    ShortcutSession {
        id: compositor
        trackWindows: true
        onWindowSnapshot: windows => root.nativeWindows = windows
    }
    function launch(command, prepareCommand) {
        focusRetry.stop();
        attempts = 0;
        pendingCommand = prepareCommand && prepareCommand.length ? command : [];
        launchStarted(root.desktopId);
        Quickshell.execDetached(pendingCommand.length ? prepareCommand : command);
        focusRetry.start();
    }
    Timer {
        id: focusRetry
        interval: 150
        repeat: true
        onTriggered: {
            root.attempts++;
            const id = root.desktopId.toLowerCase().replace(/\.desktop$/, "");
            const nativeWindow = root.nativeWindows.find(item => item.appId.toLowerCase().replace(/\.desktop$/, "") === id);
            const window = root.windows.find(item => item.appId.toLowerCase().replace(/\.desktop$/, "") === id);
            if (nativeWindow || window) {
                if (root.pendingCommand.length) {
                    Quickshell.execDetached(root.pendingCommand);
                    root.pendingCommand = [];
                }
                if (nativeWindow && compositor.nativeProtocol) {
                    if (nativeWindow.focused) {
                        stop();
                        return;
                    }
                    // This client has no activation context from the launch
                    // request, so a direct window.focus call would be denied.
                    // Keep observing the app while its own activation request
                    // is handled by Gnoblin.
                } else if (nativeWindow && compositor.connected) {
                    // GNOME compatibility sessions still use the legacy
                    // shell activation request.
                    if (nativeWindow.focused) {
                        stop();
                        return;
                    }
                    compositor.activateWindow(nativeWindow.id);
                } else if (window) {
                    if (window.activated) {
                        stop();
                        return;
                    }
                    window.minimized = false;
                    window.activate();
                }
            }
            if (root.attempts >= 60) {
                stop();
                root.pendingCommand = [];
                if (!nativeWindow && !window)
                    root.failed("Calendar did not open");
            }
        }
    }
}
