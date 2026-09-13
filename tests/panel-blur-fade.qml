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
        WlrLayershell.layer: WlrLayer.Background
        color: "black"
        Grid {
            columns: 160
            Repeater {
                model: 160 * 100
                Rectangle {
                    required property int index
                    width: 8
                    height: 8
                    color: (index % 160 + Math.floor(index / 160)) % 2 ? "#eeeeee" : "#222222"
                }
            }
        }
    }
    ShellPopup {
        id: popup
        initialRevealScale: 1
        popupWidth: 240
        popupHeight: 160
        Text {
            text: "Panel fade"
            color: Theme.text
        }
    }
    WindowSwitcher {
        id: switcher
        enabled: false
    }
    FloatingWindow {
        id: sampleWindow
        title: "Panel fade test window"
        implicitWidth: 160
        implicitHeight: 80
        visible: false
        color: "#404040"
    }
    DockTooltip {
        id: tooltip
        text: "Tooltip fade"
    }
    IpcHandler {
        target: "panels"
        function ready(): bool {
            return PopupTransitions.fadesIn() && PopupTransitions.matches(100, Easing.OutCubic, "bingux-switcher") && PopupTransitions.fadesIn("gnoblin-dock-tooltip");
        }
        function popupOpacity(): real {
            return popup.presentationOpacity;
        }
        function seedWindow(): void {
            sampleWindow.visible = true;
        }
        function present(kind: string): void {
            if (kind === "popup")
                popup.visible = true;
            if (kind === "tooltip")
                tooltip.shown = true;
            if (kind === "switcher") {
                switcher.step(false);
            }
        }
        function dismiss(kind: string): void {
            if (kind === "popup")
                popup.visible = false;
            if (kind === "tooltip")
                tooltip.shown = false;
            if (kind === "switcher")
                switcher.cancel();
        }
    }
}
