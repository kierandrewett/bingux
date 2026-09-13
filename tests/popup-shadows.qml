import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "../shell/bingux"

ShellRoot {
    PanelWindow {
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        color: "#8090a0"
        WlrLayershell.layer: WlrLayer.Background
    }
    SearchOverlay {
        id: search
    }
    EmojiPicker {
        id: emoji
        shortcutEnabled: false
    }
    WindowSwitcher {
        id: switcher
        enabled: false
    }
    property var current: null
    function find(item) {
        if (item.objectName === "popupShadow" && item.surface.visible && item.surface.opacity > 0)
            return item;
        for (const child of item.children || []) {
            const found = find(child);
            if (found)
                return found;
        }
        return null;
    }
    IpcHandler {
        target: "shadows"
        function present(kind: string): void {
            search.closeSearch();
            emoji.close();
            switcher.cancel();
            if (kind === "search") {
                search.showSearch();
                current = search.contentItem;
            } else if (kind === "emoji") {
                emoji.showPicker();
                current = emoji.nativeWindow.contentItem;
            } else {
                switcher.refresh([
                    {
                        id: "1",
                        appId: "org.gnome.Calculator",
                        title: "Calculator",
                        focused: true,
                        geometry: {
                            width: 800,
                            height: 600
                        }
                    },
                    {
                        id: "2",
                        appId: "org.gnome.TextEditor",
                        title: "Notes",
                        geometry: {
                            width: 800,
                            height: 600
                        }
                    }
                ]);
                switcher.step(false);
                current = switcher.nativeWindow.contentItem;
            }
        }
        function shadow(enabled: bool): string {
            const item = find(current);
            if (!item)
                return JSON.stringify({
                    error: "missing shadow"
                });
            item.enabledShadow = enabled;
            return JSON.stringify({
                layers: item.layers.length,
                visible: item.visible,
                opacity: item.opacity
            });
        }
    }
}
