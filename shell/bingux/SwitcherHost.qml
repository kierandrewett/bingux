import QtQuick
import Quickshell

// The popout lives in a Scope so one process can host it beside the others.
Scope {
    UiSession {
        id: session
        sessionName: "switcher"
        state: ({
                visible: switcher.active,
                shown: switcher.shown
            })
        onCommandReceived: command => {
            if (command.action === "close")
                switcher.close();
        }
    }
    WindowSwitcher {
        id: switcher
        onCloseRequested: name => session.command(name, {
                action: "close"
            })
        onOpening: {
            session.command("search", {
                action: "close"
            });
            session.command("capture", {
                action: "close"
            });
            session.command("emoji", {
                action: "close"
            });
        }
        activeStreams: session.states.desktop?.activeStreams || []
        notifications: {
            const entries = [];
            for (const group of session.states.desktop?.notificationGroups || [])
                for (let index = 0; index < group.count; index++)
                    entries.push(group);
            return entries;
        }
    }
}
