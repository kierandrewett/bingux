import QtQuick
import Quickshell
import QMLTermWidget 2.0

QMLTermWidget {
    id: root
    objectName: "sidebarTerminal"
    property bool shellRunning: true
    property int shellPid: 0
    font: Qt.font({
        family: "DejaVu Sans Mono",
        pointSize: Theme.fontSize * 0.75
    })
    colorScheme: "Bingux"
    // The FBO paint path can crash when a retained terminal becomes visible again.
    // Paint its image on the CPU and let the scene graph composite the texture.
    useFBORendering: false
    focus: true
    Accessible.name: "Sidebar terminal"

    function focusTerminal() {
        forceActiveFocus();
    }
    function applyTheme() {
        setBackgroundColor(Theme.barBackground);
        // Older loaded plugins retain the matching surface colour until reload.
        if ("backgroundOpacity" in root) {
            root["backgroundOpacity"] = 0;
            root.fillColor = "transparent";
        }
        setForegroundColor(Theme.text);
        if ("selectionColor" in root) root["selectionColor"] = Theme.textSelection;
    }

    session: QMLTermSession {
        id: terminalSession
        initialWorkingDirectory: Quickshell.env("HOME")
        shellProgram: Quickshell.env("SHELL") || "/bin/sh"
        onStarted: root.shellPid = getShellPID()
        onFinished: {
            root.shellRunning = false;
            root.shellPid = 0;
        }
    }
    Component.onCompleted: {
        applyTheme();
        terminalSession.startShellProgram();
    }
    Connections {
        target: Theme
        function onBarBackgroundChanged() {
            root.applyTheme();
        }
        function onTextChanged() {
            root.applyTheme();
        }
        function onTextSelectionChanged() { root.applyTheme(); }
    }
    // QMLTermWidget emits finished for normal exits, but not signal exits.
    Timer {
        interval: 1000
        repeat: true
        running: root.shellRunning
        onTriggered: {
            if (terminalSession.getShellPID() === 0) {
                root.shellRunning = false;
                root.shellPid = 0;
            }
        }
    }
    Shortcut {
        sequence: "Ctrl+Shift+C"
        enabled: root.activeFocus
        onActivated: root.copyClipboard()
    }
    Shortcut {
        sequence: "Ctrl+Shift+V"
        enabled: root.activeFocus
        onActivated: root.pasteClipboard()
    }
}
