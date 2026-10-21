import Quickshell

// The wallpaper's context menu. It opens Bingux Settings pages.
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
            triggered: () => Quickshell.execDetached([Python.python, Python.applicationSpawn, "--", Python.settings].concat(page ? [page] : []))
        };
    }

    preferredX: requestedPosition.x
    preferredY: requestedPosition.y
    compactRows: true
    actions: [settingsAction("Change Background…", "background"), settingsAction("Display Settings", "display")]
}
