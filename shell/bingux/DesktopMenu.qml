import Quickshell

// The wallpaper's context menu. It follows GNOME's desktop menu and opens GNOME Settings pages.
TrayMenu {
    id: root

    property point requestedPosition: Qt.point(0, 0)

    function settingsAction(text, page) {
        return {
            text,
            enabled: true,
            isSeparator: false,
            hasChildren: false,
            checkState: Qt.Unchecked,
            triggered: () => Quickshell.execDetached(["env", "XDG_CURRENT_DESKTOP=GNOME", "gnome-control-center"].concat(page ? [page] : []))
        };
    }

    preferredX: requestedPosition.x
    preferredY: requestedPosition.y
    compactRows: true
    actions: [settingsAction("Change Background…", "background"), settingsAction("Display Settings", "display"), settingsAction("Settings", "")]
}
