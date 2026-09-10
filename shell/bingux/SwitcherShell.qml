//@ pragma UseQApplication
import QtQuick
import Quickshell

ShellRoot {
    Connections { target: Quickshell; function onReloadCompleted() { Quickshell.inhibitReloadPopup(); } }
    UiSession {
        id: session
        sessionName: "switcher"
        state: ({visible: switcher.active, shown: switcher.shown})
        onCommandReceived: command => { if (command.action === "close") switcher.close(); }
    }
    WindowSwitcher {
        id: switcher
        onOpening: {
            session.command("search", {action: "close"});
            session.command("capture", {action: "close"});
            session.command("emoji", {action: "close"});
        }
        activeStreams: session.states.desktop?.activeStreams || []
        notifications: {
            const entries = [];
            for (const group of session.states.desktop?.notificationGroups || [])
                for (let index = 0; index < group.count; index++) entries.push(group);
            return entries;
        }
    }
}
