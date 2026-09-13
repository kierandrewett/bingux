import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    WindowMenu {
        id: menu
    }
    IpcHandler {
        target: "menu"
        function open(payload: string): void {
            menu.showRequest(payload);
        }
        function state(): string {
            return JSON.stringify({
                visible: menu.visible,
                retained: menu.retained,
                opacity: menu.presentationOpacity,
                revealScale: menu.revealScale,
                origin: [menu.revealOriginX, menu.revealOriginY],
                window: menu.windowId,
                x: menu.panelX,
                y: menu.panelY,
                actions: menu.actions.map(a => a.text)
            });
        }
    }
}
