//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import Bingux.Wayland
import "."

// Renders the Dock in desktop edit mode with no apps and no dock layout. The
// editor stub is what DesktopEditing reads when the desktop editor is open;
// the empty-dock hint (objectName dockEmptyHint) must show the drop prompt.
ShellRoot {
    id: root

    Component.onCompleted: {
        SessionContext.enter("desktop");
        DesktopEditing.editor = stubEditor;
    }

    QtObject {
        id: stubEditor
        property bool visible: true
        property var layout: ({
                dock: []
            })
        property var desktop: ({
                dockSize: 56,
                dockFolders: [],
                dockAlignment: "center",
                layout: {
                    dock: []
                }
            })
    }

    Variants {
        model: Quickshell.screens
        Dock {
            id: dock
            required property var modelData
            screen: modelData
            settings: ({
                    pinnedApps: []
                })
            WlrLayershell.layer: WlrLayer.Overlay
        }
    }
    Timer {
        interval: 6000
        running: true
        onTriggered: console.warn("DOCK_EMPTY_EDIT_DONE")
    }
}
