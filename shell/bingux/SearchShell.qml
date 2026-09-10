//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    Connections { target: Quickshell; function onReloadCompleted() { Quickshell.inhibitReloadPopup(); } }
    UiSession {
        id: session
        sessionName: "search"
        state: ({visible: search.visible, revealCompanions: search.chromeRevealed,
            surface: search.visible ? "bingux-search" : "bingux-search-chrome", companionsAbove: true,
            companions: ["bingux-top-bar", "bingux-dock", "bingux-panel-outline"]})
        onCommandReceived: command => {
            if (command.action === "open") search.showSearch();
            else if (command.action === "hide") search.closeSearch(false, true);
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
        function beginExternalLaunch(id, name) {
            session.command("desktop", {action: "launch-start", id, name});
            return true;
        }
        function endExternalLaunch(id) {
            session.command("desktop", {action: "launch-end", id});
        }
    }
    // Keep a non-interactive overlay anchor while Super hides only search.
    // The compositor raises the real panel buffers relative to this surface.
    PanelWindow {
        screen: search.screen
        visible: search.chromeRevealed && !search.visible
        implicitWidth: 1
        implicitHeight: 1
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        mask: Region {}
        anchors { top: true; left: true }
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-search-chrome"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }
    SearchOverlay {
        id: search
        onVisibleChanged: if (visible) {
            session.command("switcher", {action: "close"});
            session.command("capture", {action: "close"});
            session.command("emoji", {action: "close"});
        }
        dockView: dockProxy
        onSettingsRequested: session.command("desktop", {action: "settings"})
    }
}
