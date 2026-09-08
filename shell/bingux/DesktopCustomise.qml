import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "DesktopLayout.js" as DesktopLayout

PanelWindow {
    id: root
    readonly property alias preview: canvas
    required property var settings
    visible: false
    color: Theme.barBackground
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    exclusiveZone: 0
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "bingux-customise"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    property var desktop: ({})
    property var layout: DesktopLayout.defaults()
    property string tab: "Widgets"
    property string selectedWidget: ""
    property string draggedId: ""
    property string hoverZone: ""
    property int hoverIndex: 0
    property int hoverVisualIndex: 0
    property point pointer: Qt.point(0, 0)
    property string wallpaper: ""
    property bool applying: false
    property var originalDesktop: ({})
    property bool originalDirty: false
    function cancel() {
        if (applying) { settings.draft = Object.assign({}, settings.draft, {desktop: originalDesktop}); settings.dirty = originalDirty; }
        applying = false; visible = false;
    }
    readonly property var zones: [leftZone, centerZone, rightZone, dockZone, sidebarZone]
    readonly property var available: DesktopLayout.widgets.filter(w => !DesktopLayout.zone(layout, w.id))
    function open() {
        desktop = JSON.parse(JSON.stringify(settings.draft.desktop));
        originalDesktop = JSON.parse(JSON.stringify(desktop)); originalDirty = settings.dirty;
        desktop.sidebarEdge = desktop.sidebarEdge || settings.currentSidebarEdge;
        layout = JSON.parse(JSON.stringify(desktop.layout || settings.currentLayout || DesktopLayout.defaults()));
        selectedWidget = ""; draggedId = ""; applying = false; tab = "Widgets";
        visible = true;
        wallpaperReader.running = true;
        canvas.forceActiveFocus();
    }
    function change(key, value) { desktop = Object.assign({}, desktop, {[key]: value}); }
    function put(id, target, index) {
        layout = DesktopLayout.move(layout, id, target, index);
        if (target === "dock") change("dock", true);
        if (id === "metrics" && target !== "palette") change("metrics", true);
        if (target === "sidebar") change("sidebar", true);
    }
    function drag(id, source, x, y) {
        draggedId = id;
        pointer = source.mapToItem(canvas, x, y);
        hoverZone = "";
        for (const area of zones.concat([palette])) {
            const p = canvas.mapToItem(area, pointer.x, pointer.y);
            if (p.x < 0 || p.y < 0 || p.x > area.width || p.y > area.height) continue;
            if (!DesktopLayout.accepts(id, area.zoneName)) continue;
            hoverZone = area.zoneName;
            if (area === palette) { hoverIndex = 0; break; }
            const coordinate = area.vertical ? p.y - 26 : p.x - 8;
            const all = layout[area.zoneName];
            const items = all.filter(value => value !== id);
            const slot = area.vertical ? 40 : area.chipWidth + 4;
            hoverIndex = items.filter(value => coordinate > (all.indexOf(value) + 0.5) * slot).length;
            hoverVisualIndex = hoverIndex < items.length ? all.indexOf(items[hoverIndex]) : all.length;
            break;
        }
    }
    function release() {
        if (draggedId && hoverZone) put(draggedId, hoverZone, hoverIndex);
        draggedId = ""; hoverZone = "";
    }
    function apply() {
        settings.update("desktop", "layout", layout);
        for (const key of Object.keys(desktop)) if (key !== "layout") settings.update("desktop", key, desktop[key]);
        applying = true;
        settings.save();
    }
    Connections {
        target: root.settings
        function onSaved() { if (root.applying) { root.applying = false; root.visible = false; } }
    }
    Process {
        id: wallpaperReader
        command: root.settings.helper.concat(["wallpaper"])
        stdout: StdioCollector { onStreamFinished: { try { root.wallpaper = JSON.parse(text).wallpaper || ""; } catch (_) {} } }
    }
    component Chip: Item {
        id: chip
        objectName: "customise-widget-" + widgetId
        required property string widgetId
        property bool compact: false
        readonly property var info: DesktopLayout.widget(widgetId)
        width: compact ? 44 : 146
        height: 36
        opacity: root.draggedId === widgetId ? 0.35 : 1
        Rectangle { anchors.fill: parent; radius: 7; color: root.selectedWidget === chip.widgetId ? Theme.selection : chipMouse.containsMouse ? Theme.hover : Theme.elevated; border.width: chip.activeFocus ? 1 : 0; border.color: Theme.accent }
        RowLayout {
            anchors.fill: parent; anchors.margins: 8; spacing: 8
            SymbolicIcon { implicitSize: 18; source: Quickshell.iconPath(chip.info?.icon || "application-x-executable-symbolic"); color: Theme.text }
            Text { visible: !chip.compact || ["clock", "keyboard", "metrics"].includes(chip.widgetId); Layout.fillWidth: true; text: !chip.compact ? chip.info?.label || "" : chip.widgetId === "clock" ? Qt.formatTime(new Date(), "hh:mm") : chip.widgetId === "keyboard" ? "EN" : "12%"; elide: Text.ElideRight; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
        }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: info?.label || "Widget"
        Accessible.description: "Select, then choose a destination. You can also drag this widget."
        Keys.onSpacePressed: root.selectedWidget = widgetId
        Keys.onReturnPressed: root.selectedWidget = widgetId
        MouseArea {
            id: chipMouse
            anchors.fill: parent; hoverEnabled: true
            preventStealing: true
            cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
            property point start
            onPressed: mouse => { start = Qt.point(mouse.x, mouse.y); root.selectedWidget = chip.widgetId; }
            onPositionChanged: mouse => { if (pressed && (root.draggedId || Math.abs(mouse.x - start.x) + Math.abs(mouse.y - start.y) > 6)) root.drag(chip.widgetId, chipMouse, mouse.x, mouse.y); }
            onReleased: root.release()
            onCanceled: { root.draggedId = ""; root.hoverZone = ""; }
        }
        ShellTooltip { visible: chipMouse.containsMouse && !root.draggedId; text: chip.info?.label || "" }
    }
    component Zone: Rectangle {
        id: area
        objectName: "customise-zone-" + zoneName
        required property string zoneName
        required property string label
        property bool vertical: false
        readonly property int chipWidth: vertical ? width - 16 : Math.max(38, Math.min(90, (width - 16) / Math.max(1, root.layout[zoneName].length) - 4))
        readonly property bool compatible: DesktopLayout.accepts(root.draggedId || root.selectedWidget, zoneName)
        radius: 10
        color: Theme.shellSurface
        border.width: 1
        border.color: root.hoverZone === zoneName ? Theme.accent : compatible ? Theme.accent : Theme.outline
        Text { x: 8; y: 5; text: area.label; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 11 }
        Flow {
            x: 8; y: 25; width: parent.width - 16; height: parent.height - 30
            spacing: 4
            flow: area.vertical ? Flow.TopToBottom : Flow.LeftToRight
            Repeater {
                model: root.layout[area.zoneName]
                Chip { required property string modelData; widgetId: modelData; width: area.chipWidth; compact: !area.vertical; height: 36 }
            }
        }
        Rectangle {
            visible: root.hoverZone === area.zoneName
            x: area.vertical ? 8 : Math.min(area.width - 5, 8 + root.hoverVisualIndex * (area.chipWidth + 4))
            y: area.vertical ? Math.min(area.height - 4, 25 + root.hoverVisualIndex * 40) : 24
            width: area.vertical ? area.width - 16 : 3
            height: area.vertical ? 3 : 38
            radius: 1; color: Theme.accent
        }
        Text { anchors.centerIn: parent; anchors.verticalCenterOffset: 10; visible: root.layout[area.zoneName].length === 0; text: "Drop widgets here"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
    }
    Item {
        id: canvas
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: { if (root.draggedId) { root.draggedId = ""; root.hoverZone = ""; } else if (!root.settings.busy) root.cancel(); }
        Image { anchors.fill: parent; source: root.wallpaper; fillMode: Image.PreserveAspectCrop; asynchronous: true }
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.35) }
        Rectangle {
            id: toolbar
            width: parent.width; height: 70; color: Theme.barBackground
            RowLayout {
                anchors.fill: parent; anchors.margins: 16; spacing: 12
                SymbolicIcon { implicitSize: 24; source: Quickshell.iconPath("preferences-desktop-display-symbolic"); color: Theme.text }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 3
                    Text { text: "Customise your desktop"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.DemiBold }
                    Text { text: "Drag widgets between the desktop and the palette. Changes are applied when you finish."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.fillWidth: true; elide: Text.ElideRight }
                }
                ActionButton { text: "Reset layout"; flat: true; enabled: !root.settings.busy; onClicked: root.layout = DesktopLayout.defaults() }
                ActionButton { objectName: "customiseCancel"; text: "Cancel"; enabled: !root.settings.busy; onClicked: root.cancel() }
                ActionButton { objectName: "customiseApply"; text: root.settings.busy ? "Applying…" : "Apply and finish"; enabled: !root.settings.busy; onClicked: root.apply() }
            }
        }
        Zone { id: leftZone; zoneName: "top-left"; label: "Top bar · Left"; x: 20; y: 86; width: (parent.width - 64) * 0.23; height: 72 }
        Zone { id: centerZone; zoneName: "top-center"; label: "Top bar · Centre"; x: leftZone.x + leftZone.width + 12; y: 86; width: (parent.width - 64) * 0.23; height: 72 }
        Zone { id: rightZone; zoneName: "top-right"; label: "Top bar · Right"; x: centerZone.x + centerZone.width + 12; y: 86; width: (parent.width - 64) * 0.54; height: 72 }
        Zone {
            id: sidebarZone; zoneName: "sidebar"; label: "Sidebar panels"; vertical: root.desktop.sidebarEdge !== "top"
            x: root.desktop.sidebarEdge === "left" || !vertical ? 20 : parent.width - width - 20
            y: 178; width: vertical ? 166 : parent.width - 40; height: vertical ? Math.min(parent.height - 310, 285) : 72
            opacity: root.desktop.sidebar === false ? 0.5 : 1
        }
        Rectangle {
            id: palette
            objectName: "customisePalette"
            readonly property string zoneName: "palette"
            x: Math.round((canvas.width - width) / 2) + (root.desktop.sidebarEdge === "top" ? 0 : root.desktop.sidebarEdge === "left" ? 80 : -80)
            y: root.desktop.sidebarEdge === "top" ? 266 : 184; width: Math.min(660, canvas.width - 246); height: Math.max(180, canvas.height - y - 140)
            radius: Theme.radius; color: Theme.barBackground
            border.width: root.hoverZone === "palette" ? 2 : 0; border.color: Theme.accent
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 20; spacing: 12
                RowLayout {
                    Repeater { model: ["Widgets", "Dock", "Sidebar"]; ActionButton { required property string modelData; text: modelData; flat: root.tab !== modelData; onClicked: root.tab = modelData } }
                    Item { Layout.fillWidth: true }
                }
                Flickable {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    contentWidth: width; contentHeight: options.implicitHeight; clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    ColumnLayout {
                        id: options; width: parent.width; spacing: 14
                        SettingsHeading { visible: root.tab === "Widgets"; title: "Widgets"; description: "Drag a widget into a highlighted area. Widgets already placed will move to their new position." }
                        Flow {
                            visible: root.tab === "Widgets"; Layout.fillWidth: true; spacing: 8
                            Repeater {
                                model: DesktopLayout.widgets
                                Column {
                                    required property var modelData
                                    width: 146; spacing: 3
                                    Chip { widgetId: modelData.id }
                                    Text { x: 8; text: { const zone = DesktopLayout.zone(root.layout, parent.modelData.id); return zone.startsWith("top-") ? "In top bar" : zone === "dock" ? "In dock" : zone === "sidebar" ? "In sidebar" : "Not placed"; } color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 11 }
                                }
                            }
                        }
                        ColumnLayout {
                            visible: root.tab === "Widgets" && root.selectedWidget !== ""; Layout.fillWidth: true; spacing: 8
                            SettingsHeading { title: "Move " + (DesktopLayout.widget(root.selectedWidget)?.label || "widget"); description: "You can also choose a destination without dragging." }
                            Flow {
                                Layout.fillWidth: true; spacing: 4
                                Repeater {
                                    model: [{id: "top-left", label: "Top left"}, {id: "top-center", label: "Top centre"}, {id: "top-right", label: "Top right"}, {id: "dock", label: "Dock"}, {id: "sidebar", label: "Sidebar"}, {id: "palette", label: "Remove"}]
                                    ActionButton { required property var modelData; objectName: "customise-destination-" + modelData.id; text: modelData.label; enabled: DesktopLayout.accepts(root.selectedWidget, modelData.id); onClicked: root.put(root.selectedWidget, modelData.id, root.layout[modelData.id]?.length || 0) }
                                }
                            }
                        }
                        ColumnLayout {
                            visible: root.tab === "Dock"; Layout.fillWidth: true; spacing: 16
                            SettingsHeading { title: "Dock"; description: "Preview its size and position, then choose how app icons respond." }
                            ControlRow { title: "Show dock"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.dock !== false; onToggleRequested: root.change("dock", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Alignment"; choices: ["Left", "Centre", "Right"]; values: ["left", "center", "right"]; value: root.desktop.dockAlignment || "center"; onChosen: value => root.change("dockAlignment", value) }
                            RowLayout {
                                Text { text: "Icon size"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                SeekSlider { Layout.fillWidth: true; from: 32; to: 80; stepSize: 4; value: root.desktop.dockSize || 56; onMoved: root.change("dockSize", Math.round(value)); Accessible.name: "Dock icon size" }
                                Text { text: (root.desktop.dockSize || 56) + " px"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            }
                            SettingsChoice { label: "Click"; choices: ["Focus or minimise", "Always focus", "New window"]; values: ["toggle", "focus", "launch"]; value: root.desktop.dockClick || "toggle"; onChosen: value => root.change("dockClick", value) }
                            SettingsChoice { label: "Middle click"; choices: ["New window", "Close windows", "Do nothing"]; values: ["launch", "close", "none"]; value: root.desktop.dockMiddleClick || "launch"; onChosen: value => root.change("dockMiddleClick", value) }
                            SettingsChoice { label: "Mouse wheel and trackpad"; choices: ["Switch windows", "Do nothing"]; values: ["cycle", "none"]; value: root.desktop.dockScroll || "cycle"; onChosen: value => root.change("dockScroll", value) }
                            SettingsChoice { label: "Scroll direction"; choices: ["Normal", "Reverse"]; values: ["natural", "reverse"]; value: root.desktop.dockScrollDirection || "natural"; onChosen: value => root.change("dockScrollDirection", value) }
                        }
                        ColumnLayout {
                            visible: root.tab === "Sidebar"; Layout.fillWidth: true; spacing: 16
                            SettingsHeading { title: "Sidebar"; description: "Drag panel widgets to change their order. Keep at least one panel." }
                            ControlRow { title: "Show sidebar"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.sidebar !== false; onToggleRequested: root.change("sidebar", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Screen edge"; choices: ["Left", "Top", "Right"]; values: ["left", "top", "right"]; value: root.desktop.sidebarEdge || "right"; onChosen: value => root.change("sidebarEdge", value) }
                        }
                        Text { visible: root.settings.status !== "" && root.settings.status !== "Saved"; text: root.settings.status; color: Theme.warning; Layout.fillWidth: true; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                    }
                }
            }
        }
        Row {
            id: mockDock
            x: root.desktop.dockAlignment === "left" ? 20 : root.desktop.dockAlignment === "right" ? parent.width - width - 20 : (parent.width - width) / 2
            y: parent.height - height - 20; spacing: 8
            opacity: root.desktop.dock === false ? 0.5 : 1
            Behavior on x { NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
            Rectangle {
                width: apps.width + 24; height: Math.max(72, (root.desktop.dockSize || 56) + 24); radius: Theme.shellRadius; color: Theme.shellSurface
                Row {
                    id: apps; anchors.centerIn: parent; spacing: 8
                    Repeater { model: ["firefox", "org.gnome.Nautilus", "org.gnome.Console"]; Image { required property string modelData; width: root.desktop.dockSize || 56; height: width; source: Quickshell.iconPath(modelData, "application-x-executable"); sourceSize.width: width * 2; sourceSize.height: height * 2; fillMode: Image.PreserveAspectFit } }
                }
            }
            Zone { id: dockZone; zoneName: "dock"; label: "Dock widgets"; width: Math.max(190, root.layout.dock.length * 48 + 16); height: 76 }
        }
        Rectangle {
            visible: root.draggedId !== ""; x: root.pointer.x + 12; y: root.pointer.y + 12
            width: dragLabel.implicitWidth + 28; height: 40; radius: 8; color: Theme.elevated; border.width: 1; border.color: Theme.accent
            Text { id: dragLabel; anchors.centerIn: parent; text: DesktopLayout.widget(root.draggedId)?.label || ""; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
        }
    }
}
