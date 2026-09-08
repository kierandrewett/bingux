//@ pragma UseQApplication
import QtQuick
import Quickshell

ShellRoot {
    Connections { target: Quickshell; function onReloadCompleted() { Quickshell.inhibitReloadPopup(); } }
    UiSession {
        id: session
        sessionName: "search"
        state: ({visible: search.visible, surface: "bingux-search",
            companions: ["bingux-top-bar", "bingux-dock", "bingux-panel-outline"]})
        onCommandReceived: command => {
            if (command.action === "open") search.showSearch();
            else if (command.action === "close") search.closeSearch();
            else if (command.action === "toggle") search.toggleSearch();
        }
    }
    QtObject {
        id: dockProxy
        function normaliseAppId(id) { return id.endsWith(".desktop") ? id.slice(0, -8) : id; }
        function desktopEntryFor(id) { return DesktopEntries.byId(id) || DesktopEntries.byId(id + ".desktop") || DesktopEntries.heuristicLookup(id); }
        function isPinned(group) {
            return (session.states.desktop?.pinnedApps || []).includes(normaliseAppId(group.desktopEntry?.id || group.id));
        }
        function setPinned(group, pinned) {
            session.command("desktop", {action: "pin", id: group.desktopEntry?.id || group.id, pinned});
        }
    }
    SearchOverlay {
        id: search
        onVisibleChanged: if (visible) session.command("switcher", {action: "close"})
        dockView: dockProxy
        onSettingsRequested: session.command("desktop", {action: "settings"})
    }
}
