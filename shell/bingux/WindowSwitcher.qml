import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets

Scope {
    id: root
    property bool enabled: true
    property int showDelay: 80
    property bool active: false
    property bool shown: false
    property var history: []
    property var liveWindows: []
    property var windows: []
    property int selected: 0
    property var activeScreen: Quickshell.screens[0] || null
    readonly property var selectedWindow: active && windows.length ? windows[selected] : null
    readonly property int visibleCount: Math.max(1, Math.min(9, Math.floor(((activeScreen ? activeScreen.width : 1280) - 64) / 80)))
    readonly property int firstVisible: Math.min(Math.max(0, selected - Math.floor(visibleCount / 2)), Math.max(0, windows.length - visibleCount))
    readonly property var visibleWindows: windows.slice(firstVisible, firstVisible + visibleCount)
    signal opening()

    function refresh(snapshot) {
        const live = snapshot.filter(window => window.title && (!window.parent || !snapshot.some(parent => parent.id === window.parent)));
        liveWindows = live;
        const previousIds = history.map(window => window.id);
        history = history.map(window => live.find(next => next.id === window.id)).filter(Boolean)
            .concat(live.filter(window => previousIds.indexOf(window.id) < 0).sort((a, b) => b.lastUserTime - a.lastUserTime));
        const focused = live.find(window => window.focused);
        if (focused) history = [focused].concat(history.filter(window => window.id !== focused.id));
        if (!active) return;
        const previous = selectedWindow;
        windows = windows.map(window => live.find(next => next.id === window.id)).filter(Boolean);
        const retained = previous ? windows.findIndex(window => window.id === previous.id) : -1;
        selected = retained >= 0 ? retained : Math.max(0, Math.min(selected, windows.length - 1));
        if (!windows.length) cancel();
    }
    function step(backwards) {
        if (!active) {
            windows = history.slice();
            if (!windows.length) { shortcuts.end(); return; }
            active = true;
            selected = backwards ? windows.length - 1 : windows.length > 1 ? 1 : 0;
            const focused = history.find(window => window.focused);
            activeScreen = focused && focused.monitor ? Quickshell.screens.find(screen =>
                screen.x === focused.monitor.x && screen.y === focused.monitor.y) || Quickshell.screens[0]
                : Quickshell.screens[0] || null;
            opening();
            reveal.restart();
        } else {
            selected = (selected + (backwards ? -1 : 1) + windows.length) % windows.length;
        }
    }
    function cancel() {
        reveal.stop();
        active = false;
        shown = false;
        windows = [];
    }
    function close() { cancel(); shortcuts.end(); }
    function finish() {
        const window = selectedWindow;
        cancel();
        if (window && liveWindows.some(live => live.id === window.id)) shortcuts.activateWindow(window.id);
    }
    function iconFor(window) {
        if (!window) return "";
        const app = DesktopEntries.byId(window.appId) || DesktopEntries.byId(window.appId + ".desktop");
        return Quickshell.iconPath(app && app.icon ? app.icon : "application-x-executable", "application-x-executable");
    }
    Timer { id: reveal; interval: root.showDelay; onTriggered: if (root.active) root.shown = true }
    ShortcutSession {
        id: shortcuts
        trackWindows: true
        onWindowSnapshot: function(windows) { root.refresh(windows); }
        enabled: root.enabled
        bindings: [
            {id: "switcher-forward", accelerator: "<Alt>Tab", hold: 8},
            {id: "switcher-backward", accelerator: "<Alt><Shift>Tab", hold: 8},
            {id: "switcher-super-forward", accelerator: "<Super>Tab", hold: 67108864},
            {id: "switcher-super-backward", accelerator: "<Super><Shift>Tab", hold: 67108864}
        ]
        onActivated: function(id, first, modifiers) { root.step(id.indexOf("backward") >= 0); }
        onReleased: root.finish()
        onCancelled: root.cancel()
        onFailed: function(message) { console.warn("bingux-switcher: " + message); }
        onPointerPressed: function(x, y, button) {
            const origin = iconRow.mapToItem(window.contentItem, 0, 0);
            x -= root.activeScreen.x + origin.x;
            y -= root.activeScreen.y + origin.y;
            if (root.shown && button === 1 && x >= 0 && x < iconRow.width && y >= 0 && y < 76) {
                root.selected = root.firstVisible + Math.floor(x / 80);
                root.finish();
                shortcuts.end();
            } else root.close();
        }
        onKeyPressed: function(key, modifiers) {
            if (key === 65307) { root.cancel(); shortcuts.end(); }
            else if (key === 65293 || key === 65421) { root.finish(); shortcuts.end(); }
            else if (key === 65361 || key === 65056) root.step(true);
            else if (key === 65363 || key === 65289) root.step(key === 65289 && (modifiers & 1) !== 0);
        }
    }
    FileView {
        path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/switcher.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const settings = JSON.parse(text());
                if (typeof settings !== "object" || settings === null || Array.isArray(settings)
                    || Object.keys(settings).some(key => ["enabled", "showDelay"].indexOf(key) < 0)
                    || (settings.enabled !== undefined && typeof settings.enabled !== "boolean")
                    || (settings.showDelay !== undefined && (!Number.isInteger(settings.showDelay) || settings.showDelay < 0 || settings.showDelay > 500)))
                    throw new Error("Invalid switcher settings");
                root.cancel();
                shortcuts.end();
                root.enabled = settings.enabled === undefined ? true : settings.enabled;
                root.showDelay = settings.showDelay === undefined ? 80 : settings.showDelay;
            } catch (error) { console.warn("bingux-switcher: keeping previous settings: " + error); }
        }
    }
    IpcHandler {
        target: "switcher"
        function status(): string {
            return JSON.stringify({active: root.active, shown: root.shown, ready: shortcuts.ready,
                selected: root.selectedWindow ? root.selectedWindow.title : null,
                windows: root.windows.map(window => window.title)});
        }
        function close(): void { root.close(); }
    }
    PanelWindow {
        id: window
        screen: root.activeScreen
        visible: root.shown
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "bingux-switcher"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors { top: true; bottom: true; left: true; right: true }
        mask: Region { width: 0; height: 0 }
        Rectangle {
            anchors.centerIn: parent
            width: Math.max(272, iconRow.implicitWidth + 32)
            height: 140
            radius: 16
            color: Qt.rgba(Theme.surface.r, Theme.surface.g, Theme.surface.b, 0.96)
            border.width: 1
            border.color: Theme.outline
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: 12
                RowLayout {
                    id: iconRow
                    Layout.alignment: Qt.AlignHCenter
                    spacing: 4
                    Repeater {
                        model: root.visibleWindows
                        Rectangle {
                            required property var modelData
                            required property int index
                            Layout.preferredWidth: 76
                            Layout.preferredHeight: 76
                            radius: Theme.insetRadius(16, 4)
                            color: index + root.firstVisible === root.selected ? Theme.selection : "transparent"
                            border.width: index + root.firstVisible === root.selected ? 1 : 0
                            border.color: Theme.accent
                            IconImage { anchors.centerIn: parent; implicitSize: 48; source: root.iconFor(modelData) }
                        }
                    }
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    color: Theme.text
                    elide: Text.ElideRight
                    text: (root.selectedWindow ? root.selectedWindow.title : "") + "   ·   " + (root.selected + 1) + " / " + root.windows.length
                }
            }
        }
    }
}
