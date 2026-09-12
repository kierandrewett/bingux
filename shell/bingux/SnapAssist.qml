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
    // Pointer samples must not rebuild the regions and their delegates every frame.
    property var geometryContext: null
    readonly property var monitor: geometryContext ? geometryContext.monitor : null
    readonly property var area: geometryContext ? geometryContext.area : null
    readonly property var output: monitor ? Quickshell.screens.find(s => s.x === monitor.x && s.y === monitor.y) || null : null
    readonly property bool control: dragging && state && Boolean(state.modifiers & 4)
    readonly property var layouts: {
        try {
            const list = JSON.parse(preferences.layouts);
            if (Layouts.valid(list))
                return list;
        } catch (_) {}
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
        if (!area || !monitor)
            return [];
        const list = dragging && !control && !keyboardMode ? Layouts.edgeRegions(monitor, area, preferences.innerGap, preferences.outerGap) : [];
        if (!(expanded || control || keyboardMode))
            return list;
        layouts.forEach((layout, li) => layout.tiles.forEach((tile, ti) => {
                if (control && li !== Math.min(preferences.activeLayout, layouts.length - 1))
                    return;
                const hit = control ? {
                    x: area.x + tile.x * area.width,
                    y: area.y + tile.y * area.height,
                    width: tile.width * area.width,
                    height: tile.height * area.height
                } : {
                    x: pickerX + 12 + (li % columns) * 116 + tile.x * 104,
                    y: pickerY + 12 + Math.floor(li / columns) * 80 + tile.y * 60,
                    width: tile.width * 104,
                    height: tile.height * 60
                };
                list.push({
                    id: li + ":" + ti,
                    layout: li,
                    tile: ti,
                    hit,
                    target: Layouts.target(tile, area, preferences.innerGap, preferences.outerGap),
                    control
                });
            }));
        return list;
    }
    readonly property var selected: keyboardMode ? regions[keyboardSelection] || null : dragging && state ? regions.find(region => Layouts.contains(region.hit, state.x, state.y)) || null : null
    onRegionsChanged: if (dragging && state)
        connection.send({
            op: "snap-offer",
            serial: state.serial,
            regions
        })

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
    function updateGeometry(record) {
        const next = {
            monitor: record.monitor,
            area: record.area
        };
        if (JSON.stringify(next) !== JSON.stringify(geometryContext))
            geometryContext = next;
    }
    function update(record) {
        if (!record.active) {
            dragging = false;
            expanded = false;
            return;
        }
        if (!state || record.serial !== state.serial || record.monitor.id !== state.monitor.id)
            expanded = false;
        state = record;
        updateGeometry(record);
        dragging = true;
        keyboardMode = false;
        const reach = {
            x: pickerX - 48,
            y: monitor.y,
            width: pickerWidth + 96,
            height: expanded ? pickerY - monitor.y + pickerHeight + 48 : pickerY - monitor.y + 54
        };
        expanded = Layouts.contains(reach, state.x, state.y);
    }
    function close() {
        keyboardMode = false;
        expanded = false;
    }
    function commit(region) {
        if (!region || !state)
            return;
        connection.send({
            op: "snap-window",
            window: state.window,
            monitor: monitor.id,
            target: region.target
        });
        if (region.layout >= 0)
            preferences.activeLayout = region.layout;
        close();
    }
    ShortcutSession {
        id: connection
        trackWindowDrag: true
        bindings: [
            {
                id: "snap-layouts",
                accelerator: "<Super>z",
                hold: 0,
                modal: false
            }
        ]
        onWindowDrag: record => root.update(record)
        onSnapCompleted: record => {
            if (Number.isInteger(record.layout) && record.layout >= 0 && record.layout < root.layouts.length)
                preferences.activeLayout = record.layout;
        }
        onActivated: connection.send({
            op: "snap-context"
        })
        onSnapContext: record => {
            root.state = record;
            root.updateGeometry(record);
            root.dragging = false;
            root.keyboardSelection = 0;
            root.keyboardMode = true;
            root.error = "";
        }
        onCancelled: {
            root.dragging = false;
            root.close();
        }
        onFailed: message => {
            root.error = message;
            console.warn("Snapping:", message);
        }
    }
    IpcHandler {
        target: "snapping"
        function open(): void {
            connection.send({
                op: "snap-context"
            });
        }
        function close(): void {
            root.close();
        }
        function status(): string {
            return JSON.stringify({
                connected: connection.connected,
                ready: connection.ready,
                layouts: root.layouts.length,
                dragging: root.dragging,
                keyboard: root.keyboardMode,
                monitor: root.monitor,
                area: root.area,
                error: root.error
            });
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
            if (!visible && root.keyboardMode)
                root.close();
            else if (visible && root.keyboardMode)
                Qt.callLater(() => body.forceActiveFocus());
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
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                root.commit(root.selected);
            else if (event.key === Qt.Key_Escape)
                root.close();
            else
                return;
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
        // Keep the surface mapped until its last visual has faded away.
        visible: !!root.output && (root.dragging || root.keyboardMode || highlight.opacity > 0 || snapPill.opacity > 0 || guideOpacity > 0)
        property real guideOpacity: root.dragging && !root.control ? (root.selected ? .25 : .65) : 0
        Behavior on guideOpacity {
            NumberAnimation {
                duration: Theme.reducedMotion ? 0 : 160
                easing.type: Easing.OutCubic
            }
        }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-snap-preview"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        mask: Region {
            width: 0
            height: 0
        }
        Repeater {
            model: root.area ? [0, 1, 2, 3] : []
            Item {
                required property int modelData
                readonly property bool rightSide: modelData % 2 === 1
                readonly property bool bottomSide: modelData >= 2
                x: root.area && root.monitor ? root.area.x - root.monitor.x + (rightSide ? root.area.width - width - 8 : 8) : 0
                y: root.area && root.monitor ? root.area.y - root.monitor.y + (bottomSide ? root.area.height - height - 8 : 8) : 0
                width: 28
                height: 28
                opacity: preview.guideOpacity
                Rectangle {
                    width: parent.width
                    height: 3
                    radius: 1.5
                    y: parent.bottomSide ? parent.height - height : 0
                    color: Theme.accent
                }
                Rectangle {
                    width: 3
                    height: parent.height
                    radius: 1.5
                    x: parent.rightSide ? parent.width - width : 0
                    color: Theme.accent
                }
            }
        }
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
                        highlight.animateGeometry = highlight.retained !== null && highlight.opacity > 0;
                        highlight.retained = root.selected.target;
                    }
                }
            }
            // One interpolation keeps all four edges on the same animation clock.
            property rect bounds: retained && root.monitor ? Qt.rect(retained.x - root.monitor.x, retained.y - root.monitor.y, retained.width, retained.height) : Qt.rect(0, 0, 0, 0)
            x: bounds.x
            y: bounds.y
            width: bounds.width
            height: bounds.height
            opacity: root.selected ? 1 : 0
            onOpacityChanged: if (opacity === 0 && !root.dragging && !root.keyboardMode) {
                animateGeometry = false;
                retained = null;
            }
            antialiasing: true
            color: Qt.alpha(Theme.accent, .18)
            border.color: Qt.alpha(Theme.accent, .8)
            border.width: 2
            radius: Theme.cardRadius
            Behavior on bounds {
                enabled: highlight.animateGeometry && !Theme.reducedMotion
                PropertyAnimation {
                    duration: 220
                    easing.type: Easing.OutCubic
                }
            }
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : (root.selected ? 160 : 120)
                    easing.type: Easing.OutCubic
                }
            }
        }
        Rectangle {
            id: snapPill
            visible: opacity > 0
            property real reveal: root.dragging && !root.expanded && !root.control ? 1 : 0
            opacity: reveal
            transform: Translate {
                y: -10 * (1 - snapPill.reveal)
            }
            Behavior on reveal {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : 180
                    easing.type: Easing.OutCubic
                }
            }
            x: root.centerX - (root.monitor ? root.monitor.x : 0) - width / 2
            y: root.area && root.monitor ? Math.max(Theme.barHeight + 8, root.area.y - root.monitor.y + 8) : 0
            width: 56
            height: 5
            radius: 3
            color: Theme.muted
        }
    }
}
