import QtQuick
import QtCore
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "SnapLayouts.js" as Layouts

Scope {
    id: root
    property var state: null
    property bool dragging: false
    property bool keyboardMode: false
    property bool expanded: false
    property int keyboardSelection: 0
    property string error: ""
    readonly property var monitor: state ? state.monitor : null
    readonly property var area: state ? state.area : null
    readonly property var output: monitor ? Quickshell.screens.find(s => s.x === monitor.x && s.y === monitor.y) || null : null
    readonly property bool control: dragging && state && Boolean(state.modifiers & 4)
    readonly property var layouts: {
        try { const list = JSON.parse(preferences.layouts); if (Layouts.valid(list)) return list; } catch (_) {}
        return Layouts.defaults();
    }
    readonly property int columns: Math.max(1, Math.min(layouts.length, Math.floor(((area ? area.width : monitor ? monitor.width : 800) - 48) / 116)))
    readonly property int rows: Math.ceil(layouts.length / columns)
    readonly property int pickerWidth: columns * 116 + 24
    readonly property int pickerHeight: rows * 80 + 4
    readonly property real centerX: area ? area.x + area.width / 2 : monitor ? monitor.x + monitor.width / 2 : 0
    readonly property real pickerX: centerX - pickerWidth / 2
    readonly property real pickerY: monitor && area ? Math.max(monitor.y + Theme.barHeight + 8, area.y + 8) : 0
    readonly property var regions: {
        if (!state || !area || !monitor || !(expanded || control || keyboardMode)) return [];
        const list = [];
        layouts.forEach((layout, li) => layout.tiles.forEach((tile, ti) => {
            if (control && li !== Math.min(preferences.activeLayout, layouts.length - 1)) return;
            const hit = control ? {x: area.x + tile.x * area.width, y: area.y + tile.y * area.height,
                width: tile.width * area.width, height: tile.height * area.height}
                : {x: pickerX + 12 + (li % columns) * 116 + tile.x * 104,
                    y: pickerY + 12 + Math.floor(li / columns) * 80 + tile.y * 60,
                    width: tile.width * 104, height: tile.height * 60};
            list.push({id: li + ":" + ti, layout: li, tile: ti, hit,
                target: Layouts.target(tile, area, preferences.innerGap, preferences.outerGap), control});
        }));
        return list;
    }
    readonly property var selected: keyboardMode ? regions[keyboardSelection] || null
        : state ? regions.find(region => Layouts.contains(region.hit, state.x, state.y)) || null : null
    onRegionsChanged: if (dragging && state) connection.send({op: "snap-offer", serial: state.serial, regions})

    Settings {
        id: preferences
        location: "file://" + Quickshell.env("HOME") + "/.config/bingux/snapping.ini"
        property bool imported: false
        property string layouts: ""
        property int innerGap: 8
        property int outerGap: 8
        property int activeLayout: 0
    }
    Process {
        running: !preferences.imported
        command: ["python3", Qt.resolvedUrl("import-snap-layouts.py").toString().replace("file://", "")]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const saved = JSON.parse(text);
                    if (Layouts.valid(saved.layouts)) {
                        preferences.layouts = JSON.stringify(saved.layouts);
                        preferences.innerGap = Math.max(0, Math.min(64, saved.inner || 0));
                        preferences.outerGap = Math.max(0, Math.min(64, saved.outer || 0));
                    }
                } catch (_) {}
                preferences.imported = true;
                preferences.sync();
            }
        }
    }
    function update(record) {
        if (!record.active) { dragging = false; expanded = false; return; }
        if (!state || record.serial !== state.serial || record.monitor.id !== state.monitor.id) expanded = false;
        state = record;
        dragging = true;
        keyboardMode = false;
        const reach = {x: pickerX - 48, y: monitor.y, width: pickerWidth + 96,
            height: expanded ? pickerY - monitor.y + pickerHeight + 48 : pickerY - monitor.y + 54};
        expanded = Layouts.contains(reach, state.x, state.y);
    }
    function close() { keyboardMode = false; expanded = false; }
    function commit(region) {
        if (!region || !state) return;
        connection.send({op: "snap-window", window: state.window, monitor: monitor.id, target: region.target});
        preferences.activeLayout = region.layout;
        close();
    }
    ShortcutSession {
        id: connection
        trackWindowDrag: true
        bindings: [{id: "snap-layouts", accelerator: "<Super>z", hold: 0, modal: false}]
        onWindowDrag: record => root.update(record)
        onSnapCompleted: record => {
            if (Number.isInteger(record.layout) && record.layout >= 0 && record.layout < root.layouts.length)
                preferences.activeLayout = record.layout;
        }
        onActivated: connection.send({op: "snap-context"})
        onSnapContext: record => {
            root.state = record;
            root.dragging = false;
            root.keyboardSelection = 0;
            root.keyboardMode = true;
            root.error = "";
        }
        onCancelled: { root.dragging = false; root.close(); }
        onFailed: message => { root.error = message; console.warn("Snapping:", message); }
    }
    IpcHandler {
        target: "snapping"
        function open(): void { connection.send({op: "snap-context"}); }
        function close(): void { root.close(); }
        function status(): string {
            return JSON.stringify({connected: connection.connected, ready: connection.ready,
                layouts: root.layouts.length, dragging: root.dragging, keyboard: root.keyboardMode,
                monitor: root.monitor, area: root.area, error: root.error});
        }
    }
    ShellPopup {
        id: picker
        screen: root.output
        visible: !!root.output && (root.keyboardMode || (root.dragging && root.expanded && !root.control))
        keyboardInteractive: root.keyboardMode
        pointerInteractive: root.keyboardMode
        dismissOnOutsideClick: root.keyboardMode
        onVisibleChanged: {
            if (!visible && root.keyboardMode) root.close();
            else if (visible && root.keyboardMode) Qt.callLater(() => body.forceActiveFocus());
        }
        popupWidth: root.pickerWidth
        popupHeight: root.pickerHeight
        preferredX: root.monitor ? root.pickerX - root.monitor.x : 0
        preferredY: root.monitor ? root.pickerY - root.monitor.y : 0
        contentPadding: 12
        initialRevealScale: .98
        body.focus: root.keyboardMode
        body.Keys.onPressed: event => {
            if ([Qt.Key_Right, Qt.Key_Down, Qt.Key_Tab].includes(event.key)) {
                root.keyboardSelection = (root.keyboardSelection + 1) % root.regions.length;
            } else if ([Qt.Key_Left, Qt.Key_Up, Qt.Key_Backtab].includes(event.key)) {
                root.keyboardSelection = (root.keyboardSelection + root.regions.length - 1) % root.regions.length;
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.commit(root.selected);
            else if (event.key === Qt.Key_Escape) root.close();
            else return;
            event.accepted = true;
        }
        Repeater {
            model: root.layouts
            Item {
                id: miniature
                required property var modelData
                required property int index
                x: (index % root.columns) * 116
                y: Math.floor(index / root.columns) * 80
                width: 104
                height: 60
                Repeater {
                    model: miniature.modelData.tiles
                    Rectangle {
                        required property var modelData
                        required property int index
                        readonly property string regionId: miniature.index + ":" + index
                        readonly property bool chosen: root.selected && root.selected.id === regionId
                        x: modelData.x * miniature.width + 1.5
                        y: modelData.y * miniature.height + 1.5
                        width: Math.max(1, modelData.width * miniature.width - 3)
                        height: Math.max(1, modelData.height * miniature.height - 3)
                        radius: 4
                        color: chosen ? Theme.accent : Theme.elevated
                        border.width: 1
                        border.color: chosen ? Theme.accent : Theme.outline
                        Accessible.role: Accessible.Button
                        Accessible.name: "Layout " + (miniature.index + 1) + ", region " + (index + 1)
                        Accessible.onPressAction: root.commit(root.regions.find(r => r.id === regionId))
                        MouseArea {
                            anchors.fill: parent
                            enabled: root.keyboardMode
                            hoverEnabled: true
                            cursorShape: Qt.ArrowCursor
                            onEntered: root.keyboardSelection = root.regions.findIndex(r => r.id === parent.regionId)
                            onClicked: root.commit(root.selected)
                        }
                    }
                }
            }
        }

    }
    PanelWindow {
        id: preview
        screen: root.output
        visible: !!root.output && (root.dragging || root.keyboardMode)
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-snap-preview"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors { top: true; bottom: true; left: true; right: true }
        mask: Region { width: 0; height: 0 }
        Repeater {
            model: root.control ? root.regions : []
            Rectangle {
                required property var modelData
                x: modelData.target.x - root.monitor.x
                y: modelData.target.y - root.monitor.y
                width: modelData.target.width
                height: modelData.target.height
                color: Qt.alpha(Theme.popupSurface, .25)
                radius: Theme.cardRadius
                border.width: 1
                border.color: Theme.outline
            }
        }
        Rectangle {
            id: highlight
            property var retained: null
            property bool animateGeometry: false
            Connections {
                target: root
                function onSelectedChanged() {
                    if (root.selected) {
                        highlight.animateGeometry = highlight.retained !== null;
                        highlight.retained = root.selected.target;
                    } else highlight.animateGeometry = false;
                }
                function onDraggingChanged() {
                    if (!root.dragging && !root.keyboardMode) highlight.retained = null;
                }
            }
            x: retained && root.monitor ? retained.x - root.monitor.x : 0
            y: retained && root.monitor ? retained.y - root.monitor.y : 0
            width: retained ? retained.width : 0
            height: retained ? retained.height : 0
            opacity: root.selected ? 1 : 0
            color: Qt.alpha(Theme.accent, .18)
            border.color: Qt.alpha(Theme.accent, .8)
            border.width: 2
            radius: Theme.cardRadius
            Behavior on x { enabled: highlight.animateGeometry; NumberAnimation { duration: Theme.reducedMotion ? 0 : 100; easing.type: Easing.OutCubic } }
            Behavior on y { enabled: highlight.animateGeometry; NumberAnimation { duration: Theme.reducedMotion ? 0 : 100; easing.type: Easing.OutCubic } }
            Behavior on width { enabled: highlight.animateGeometry; NumberAnimation { duration: Theme.reducedMotion ? 0 : 100; easing.type: Easing.OutCubic } }
            Behavior on height { enabled: highlight.animateGeometry; NumberAnimation { duration: Theme.reducedMotion ? 0 : 100; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            visible: root.dragging && !root.expanded && !root.control
            x: root.centerX - (root.monitor ? root.monitor.x : 0) - width / 2
            y: root.area && root.monitor ? Math.max(Theme.barHeight + 8, root.area.y - root.monitor.y + 8) : 0
            width: 56
            height: 5
            radius: 3
            color: Theme.muted
        }
    }
}
