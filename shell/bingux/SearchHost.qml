import QtQuick
import Quickshell
import Quickshell.Wayland

// The popout lives in a Scope so one process can host it beside the others.
Scope {
    UiSession {
        id: session
        sessionName: "search"
        bindings: [
            {
                id: "search",
                accelerator: "Super",
                hold: 0,
                trigger: "release",
                captureInput: true
            }
        ]
        onActivated: () => search.toggleSearch()
        state: ({
                visible: search.visible,
                acceptingKeyboard: search.acceptingKeyboard,
                revealCompanions: search.chromeRevealed,
                surface: "bingux-search",
                companionsAbove: true,
                companions: ["bingux-top-bar", "bingux-dock", "bingux-panel-outline"]
            })
        onCommandReceived: command => {
            if (command.action === "open")
                search.showSearch();
            else if (command.action === "hide")
                search.closeSearch(false, true);
            else if (command.action === "close")
                search.closeSearch();
            else if (command.action === "toggle")
                search.toggleSearch();
        }
    }
    // Ordering for the surface below. Gnoblin ignores a declaration whose
    // `visible` is false, so it cannot share the search session's state,
    // which is not visible while search is hidden.
    UiSession {
        sessionName: "search-chrome"
        state: ({
                visible: search.fullscreenUnderReveal,
                surface: "bingux-search-chrome",
                companionsAbove: true,
                companions: ["bingux-top-bar", "bingux-dock", "bingux-panel-outline"]
            })
    }
    QtObject {
        id: dockProxy
        function normaliseAppId(id) {
            return id.endsWith(".desktop") ? id.slice(0, -8) : id;
        }
        function desktopEntryFor(id) {
            return DesktopEntries.byId(id) || DesktopEntries.byId(id + ".desktop") || DesktopEntries.heuristicLookup(id);
        }
        function isPinned(group) {
            return (session.states.desktop?.pinnedApps || []).includes(normaliseAppId(group.desktopEntry?.id || group.id));
        }
        function setPinned(group, pinned) {
            session.command("desktop", {
                action: "pin",
                id: group.desktopEntry?.id || group.id,
                pinned
            });
        }
        function beginExternalLaunch(id, name) {
            session.command("desktop", {
                action: "launch-start",
                id,
                name
            });
            return true;
        }
        function endExternalLaunch(id) {
            session.command("desktop", {
                action: "launch-end",
                id
            });
        }
    }
    // Over a fullscreen window, the panels stay revealed after Super hides
    // only search. This surface covers the screen below them (see the session
    // above) and the first press on it ends the reveal, so the panels drop
    // back under the window. That press does not reach the window. The surface
    // exists only while it should take input: resizing a masked surface after
    // it mapped left the press undelivered in testing.
    PanelWindow {
        screen: search.screen
        visible: search.fullscreenUnderReveal
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onPressed: search.endChromeReveal()
        }
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-search-chrome"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }
    SearchOverlay {
        id: search
        keepChromeOnOutsideClick: true
        onVisibleChanged: if (visible) {
            session.command("switcher", {
                action: "close"
            });
            session.command("capture", {
                action: "close"
            });
            session.command("emoji", {
                action: "close"
            });
        }
        dockView: dockProxy
        dockSafeInset: typeof session.states.desktop?.dockSafeInset === "number" ? session.states.desktop.dockSafeInset : Theme.dockExclusiveHeight
    }
}
