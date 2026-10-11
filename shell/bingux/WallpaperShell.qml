//@ pragma UseQApplication
import QtQuick
import Quickshell

// The wallpaper runs in its own process so it can be mapped as soon as the
// compositor is up, long before the main shell has finished loading.
ShellRoot {
    Connections {
        target: Quickshell
        function onReloadCompleted() {
            Quickshell.inhibitReloadPopup();
        }
    }
    Wallpaper {}
}
