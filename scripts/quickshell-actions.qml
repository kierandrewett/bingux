import QtQuick
import Quickshell

ShellRoot {
    Timer {
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            const entry = DesktopEntries.byId("org.bingux.ActionIdentity");
            if (!entry)
                return;
            const actions = [];
            for (const action of entry.actions)
                actions.push({
                    id: action.id,
                    name: action.name
                });
            actions.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
            console.info("BINGUX_ACTION_IDENTITY=" + JSON.stringify({
                id: entry.id,
                actions: actions
            }));
            Qt.quit();
        }
    }
}
