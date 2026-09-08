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
    property string appFilter: ""
    property string optionsPage: ""
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
    property var originalChanges: ({})
    function cancel() {
        if (applying) { settings.draft = Object.assign({}, settings.draft, {desktop: originalDesktop}); settings.dirty = originalDirty; settings.changedSettings = originalChanges; }
        applying = false; visible = false;
    }
    readonly property var zones: [leftZone, centerZone, rightZone, appDockZone, dockZone, sidebarZone, controlTiles]
    readonly property var available: DesktopLayout.widgets.filter(w => !DesktopLayout.zone(layout, w.id))
    function appId(value) { return value.endsWith(".desktop") ? value.slice(0, -8) : value; }
    function appEntry(value) { return DesktopEntries.byId(value) || DesktopEntries.byId(value + ".desktop") || DesktopEntries.heuristicLookup(value); }
    function info(id) {
        if (!id.startsWith("app:")) return DesktopLayout.widget(id);
        const entry = appEntry(id.slice(4));
        return {id, label: entry?.name || id.slice(4), icon: entry?.icon || "application-x-executable", app: true};
    }
    readonly property var applications: DesktopEntries.applications.values.filter(entry => !appFilter || entry.name.toLowerCase().includes(appFilter.toLowerCase())).slice().sort((a, b) => a.name.localeCompare(b.name))
    readonly property var dockApplications: {
        const saved = desktop.dockApps || {pinnedApps: [], order: []};
        const order = saved.order.map(id => appId(appEntry(id)?.id || id));
        return saved.pinnedApps.slice().sort((a, b) => {
            const ai = order.indexOf(a), bi = order.indexOf(b);
            return (ai < 0 ? order.length : ai) - (bi < 0 ? order.length : bi);
        }).map(id => "app:" + id);
    }
    function accepts(id, target) {
        return id.startsWith("app:") ? ["dock-apps", "dock", "palette"].includes(target) : DesktopLayout.accepts(id, target);
    }
    function open() {
        desktop = JSON.parse(JSON.stringify(settings.draft.desktop));
        originalDesktop = JSON.parse(JSON.stringify(desktop)); originalDirty = settings.dirty;
        originalChanges = JSON.parse(JSON.stringify(settings.changedSettings));
        desktop.sidebarEdge = desktop.sidebarEdge || settings.currentSidebarEdge;
        layout = JSON.parse(JSON.stringify(desktop.layout || settings.currentLayout || DesktopLayout.defaults()));
        selectedWidget = ""; draggedId = ""; applying = false; tab = "Widgets"; optionsPage = ""; appFilter = "";
        visible = true;
        wallpaperReader.running = true;
        canvas.forceActiveFocus();
    }
    function change(key, value) { desktop = Object.assign({}, desktop, {[key]: value}); }
    function put(id, target, index) {
        if (id.startsWith("control-")) {
            if (!accepts(id, target)) return;
            const name = id.slice(8);
            const order = (desktop.controlOrder || DesktopLayout.controlOrder()).filter(value => value !== name);
            if (target !== "palette") order.splice(Math.max(0, Math.min(index, order.length)), 0, name);
            change("controlOrder", order);
            if (!["network", "bluetooth"].includes(name)) change("controlCentre", Object.assign({vpn: true, dnd: true, nightLight: false, power: false, awake: false}, desktop.controlCentre || {}, {[name]: target !== "palette"}));
            return;
        }
        if (id.startsWith("app:")) {
            if (!accepts(id, target)) return;
            const pins = dockApplications.map(value => value.slice(4)).filter(value => value !== id.slice(4));
            if (target !== "palette") pins.splice(target === "dock" ? pins.length : Math.max(0, Math.min(index, pins.length)), 0, id.slice(4));
            const oldOrder = desktop.dockApps?.order || [];
            change("dockApps", {pinnedApps: pins, order: pins.concat(oldOrder.filter(value => !pins.includes(appId(appEntry(value)?.id || value))))});
            if (target !== "palette") change("dock", true);
            return;
        }
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
            if (!accepts(id, area.zoneName)) continue;
            hoverZone = area.zoneName;
            if (area === palette) { hoverIndex = 0; break; }
            if (area === controlTiles) {
                const before = area.items.findIndex((value, index) => {
                    const item = controlRepeater.itemAt(index);
                    return value !== id && item && p.y < item.y + item.height / 2 && (p.y < item.y || p.x < item.x + item.width / 2);
                });
                const order = (desktop.controlOrder || DesktopLayout.controlOrder()).filter(value => value !== id.slice(8));
                hoverIndex = before < 0 ? order.length : order.indexOf(area.items[before].slice(8));
                break;
            }
            const coordinate = area.vertical ? p.y - area.contentTop : p.x - area.contentX;
            const all = area.items;
            const items = all.filter(value => value !== id);
            const slot = area.vertical ? area.chipHeight + 4 : area.chipWidth + 4;
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
    component DragHandle: MouseArea {
            required property string widgetId
            anchors.fill: parent; hoverEnabled: true
            preventStealing: true
            cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
            property point start
            onPressed: mouse => { start = Qt.point(mouse.x, mouse.y); root.selectedWidget = widgetId; }
            onPositionChanged: mouse => { if (pressed && (root.draggedId || Math.abs(mouse.x - start.x) + Math.abs(mouse.y - start.y) > 6)) root.drag(widgetId, this, mouse.x, mouse.y); }
            onReleased: root.release()
            onCanceled: { root.draggedId = ""; root.hoverZone = ""; }
        }
    component Chip: Item {
        id: chip
        objectName: "customise-widget-" + widgetId
        required property string widgetId
        property bool compact: false
        property bool paletteTile: false
        readonly property var info: root.info(widgetId)
        width: paletteTile ? 112 : compact ? 44 : 146
        height: paletteTile ? 88 : 36
        opacity: root.draggedId === widgetId ? 0.35 : 1
        Rectangle { anchors.fill: parent; radius: 7; color: root.selectedWidget === chip.widgetId ? Theme.selection : chipMouse.containsMouse ? Theme.hover : "transparent"; border.width: chip.activeFocus ? 1 : 0; border.color: Theme.accent }
        Item {
            id: chipIcon
            visible: !(chip.compact && chip.widgetId === "clock")
            width: chip.info?.app ? (chip.paletteTile ? 28 : Math.min(chip.width, chip.height) - 8) : chip.paletteTile ? 24 : 18
            height: width
            x: chip.paletteTile || chip.compact ? (parent.width - width) / 2 : 8
            y: chip.paletteTile ? 14 : (parent.height - height) / 2
            SymbolicIcon { anchors.fill: parent; visible: !chip.info?.app; implicitSize: parent.width; source: Quickshell.iconPath(chip.info?.icon || "application-x-executable-symbolic"); color: Theme.text }
            Image { anchors.fill: parent; visible: !!chip.info?.app; source: visible ? Quickshell.iconPath(chip.info?.icon || "application-x-executable") : ""; sourceSize: Qt.size(64, 64); fillMode: Image.PreserveAspectFit }
        }
        Text {
            visible: chip.paletteTile || !chip.compact || chip.widgetId === "clock"
            x: chip.paletteTile ? 4 : chip.compact ? 0 : 34; y: chip.paletteTile ? 48 : (parent.height - height) / 2
            width: parent.width - x - (chip.paletteTile ? 4 : 8)
            height: chip.paletteTile ? 36 : implicitHeight
            text: chip.compact && chip.widgetId === "clock" ? Qt.formatTime(new Date(), "hh:mm") : chip.info?.label || ""; textFormat: Text.PlainText
            horizontalAlignment: chip.paletteTile || chip.compact ? Text.AlignHCenter : Text.AlignLeft
            wrapMode: chip.paletteTile ? Text.WordWrap : Text.NoWrap; maximumLineCount: 2; elide: Text.ElideRight
            color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: info?.label || "Widget"
        Accessible.description: "Select, then choose a destination. You can also drag this widget."
        Keys.onSpacePressed: root.selectedWidget = widgetId
        Keys.onReturnPressed: root.selectedWidget = widgetId
        DragHandle { id: chipMouse; widgetId: chip.widgetId }
        ShellTooltip { visible: chipMouse.containsMouse && !root.draggedId; text: chip.info?.label || "" }
    }
    component Zone: Rectangle {
        id: area
        objectName: "customise-zone-" + zoneName
        required property string zoneName
        required property string label
        property bool vertical: false
        property var items: root.layout[zoneName] || []
        property int contentTop: 4
        property int chipHeight: 32
        property int chipWidth: vertical ? width - 16 : Math.max(28, Math.min(80, (width - 16) / Math.max(1, items.length) - 4))
        property string alignment: "left"
        readonly property real contentX: vertical || alignment === "left" ? 8 : alignment === "center" ? (width - items.length * (chipWidth + 4) + 4) / 2 : width - items.length * (chipWidth + 4) - 4
        readonly property bool compatible: root.accepts(root.draggedId || root.selectedWidget, zoneName)
        radius: 5
        color: "transparent"
        border.width: root.draggedId || items.length === 0 ? 1 : 0
        border.color: root.hoverZone === zoneName ? Theme.accent : compatible ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.5) : Theme.outline
        Accessible.name: label
        Flow {
            x: area.contentX; y: area.contentTop; width: parent.width - 16; height: parent.height - area.contentTop
            spacing: 4
            flow: area.vertical ? Flow.TopToBottom : Flow.LeftToRight
            Repeater {
                model: area.items
                Chip { required property string modelData; widgetId: modelData; width: area.chipWidth; compact: true; height: area.chipHeight }
            }
        }
        Rectangle {
            visible: root.hoverZone === area.zoneName
            x: area.vertical ? 8 : Math.min(area.width - 5, area.contentX + root.hoverVisualIndex * (area.chipWidth + 4))
            y: area.vertical ? Math.min(area.height - 4, area.contentTop + root.hoverVisualIndex * (area.chipHeight + 4)) : area.contentTop
            width: area.vertical ? area.width - 16 : 3
            height: area.vertical ? 3 : area.chipHeight
            radius: 1; color: Theme.accent
        }
        Text { anchors.centerIn: parent; visible: area.items.length === 0; text: "+"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 18 }
    }
    Item {
        id: canvas
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: {
            if (root.draggedId) { root.draggedId = ""; root.hoverZone = ""; }
            else if (root.optionsPage) root.optionsPage = "";
            else if (!root.settings.busy) root.cancel();
        }
        Image { anchors.fill: parent; source: root.wallpaper; fillMode: Image.PreserveAspectCrop; asynchronous: true }
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.48) }
        Rectangle { width: parent.width; height: 40; color: Theme.barBackground }
        Zone { id: leftZone; zoneName: "top-left"; label: "Top bar left"; x: 4; y: 0; width: parent.width * 0.23; height: 40 }
        Zone { id: centerZone; zoneName: "top-center"; label: "Top bar centre"; x: parent.width * 0.4; y: 0; width: parent.width * 0.2; height: 40; alignment: "center" }
        Zone { id: rightZone; zoneName: "top-right"; label: "Top bar right"; x: parent.width * 0.62; y: 0; width: parent.width * 0.38 - 4; height: 40; alignment: "right" }
        Rectangle {
            id: sidebarBackground
            readonly property bool horizontal: root.desktop.sidebarEdge === "top"
            x: root.desktop.sidebarEdge === "left" || horizontal ? 0 : parent.width - width
            y: 40; width: horizontal ? parent.width : 60; height: horizontal ? 52 : footer.y - y
            color: Theme.barBackground
            opacity: root.desktop.sidebar === false ? 0.5 : 1
            Zone {
                id: sidebarZone; zoneName: "sidebar"; label: "Sidebar panels"; vertical: !sidebarBackground.horizontal
                x: 4; y: sidebarBackground.horizontal ? 0 : 12; width: parent.width - 8; height: parent.height - (sidebarBackground.horizontal ? 0 : 24); chipHeight: 40
            }
        }
        Item {
            id: palette
            objectName: "customisePalette"
            readonly property string zoneName: "palette"
            x: root.desktop.sidebarEdge === "left" ? 84 : 24
            y: sidebarBackground.horizontal ? 116 : 68; width: Math.max(240, controlPreview.x - x - 28); height: mockDock.y - y - 16
            Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.width: root.hoverZone === "palette" ? 1 : 0; border.color: Theme.accent }
            ColumnLayout {
                anchors.fill: parent; spacing: 14
                Text { Layout.fillWidth: true; text: "Drag your favourite items into the desktop."; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
                RowLayout {
                    spacing: 4
                    Repeater { model: ["Widgets", "Apps"]; ActionButton { required property string modelData; text: modelData; flat: root.tab !== modelData; onClicked: root.tab = modelData } }
                    Item { Layout.fillWidth: true }
                }
                SettingsField { visible: root.tab === "Apps"; label: "Find an app"; placeholderText: "Search installed apps"; Layout.margins: 0; text: root.appFilter; onEdited: value => root.appFilter = value }
                GridView {
                    Layout.fillWidth: true; Layout.fillHeight: true
                    cellWidth: 124; cellHeight: 100; clip: true; cacheBuffer: 100; reuseItems: true
                    model: root.tab === "Apps" ? root.applications.map(entry => "app:" + root.appId(entry.id)) : DesktopLayout.widgets.concat(DesktopLayout.controlWidgets).map(widget => widget.id)
                    delegate: Chip { required property string modelData; widgetId: modelData; paletteTile: true }
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                }
            }
        }
        Rectangle {
            id: controlPreview
            x: parent.width - width - (root.desktop.sidebarEdge === "right" ? 84 : 24)
            y: palette.y; width: Math.min(320, canvas.width * 0.3); height: Math.min(controlContents.implicitHeight + 32, mockDock.y - y - 16)
            radius: Theme.cardRadius; color: Theme.shellSurface
            Flickable {
                anchors.fill: parent; anchors.margins: 16; clip: true
                contentWidth: width; contentHeight: controlContents.implicitHeight; boundsBehavior: Flickable.StopAtBounds
                ColumnLayout {
                    id: controlContents; width: parent.width; spacing: 12
                    RowLayout {
                        Layout.fillWidth: true
                        SymbolicIcon { implicitSize: 22; source: Quickshell.iconPath("avatar-default-symbolic"); color: Theme.text }
                        Text { Layout.fillWidth: true; text: "Control centre"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.Medium }
                        SymbolicIcon { implicitSize: 18; source: Quickshell.iconPath("system-lock-screen-symbolic"); color: Theme.text }
                    }
                    RowLayout {
                        SymbolicIcon { implicitSize: 20; source: Quickshell.iconPath("audio-volume-high-symbolic"); color: Theme.text }
                        SeekSlider { Layout.fillWidth: true; value: 0.65; enabled: false }
                    }
                    RowLayout {
                        SymbolicIcon { implicitSize: 20; source: Quickshell.iconPath("audio-input-microphone-symbolic"); color: Theme.text }
                        SeekSlider { Layout.fillWidth: true; value: 0.45; enabled: false }
                    }
                    Item {
                        Layout.fillWidth: true; implicitHeight: Math.max(84, controlTiles.implicitHeight)
                        Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.width: root.hoverZone === "control-centre" ? 1 : 0; border.color: Theme.accent }
                    GridLayout {
                        id: controlTiles
                        objectName: "customise-zone-control-centre"
                        readonly property string zoneName: "control-centre"
                        readonly property var items: (root.desktop.controlOrder || DesktopLayout.controlOrder()).filter(id => ["network", "bluetooth"].includes(id) || root.desktop.controlCentre?.[id]).map(id => "control-" + id)
                        width: parent.width; height: parent.height; columns: 2; columnSpacing: 8; rowSpacing: 8; uniformCellWidths: true
                        Repeater {
                            id: controlRepeater
                            model: controlTiles.items
                            ControlRow {
                                required property string modelData
                                title: root.info(modelData).label; iconName: root.info(modelData).icon; tileLayout: true; compactTile: true; rowInteractive: false
                                objectName: "customise-control-" + modelData
                                opacity: root.draggedId === modelData ? 0.35 : 1
                                activeFocusOnTab: true
                                Keys.onSpacePressed: root.selectedWidget = modelData
                                Keys.onReturnPressed: root.selectedWidget = modelData
                                DragHandle { widgetId: parent.modelData }
                            }
                        }
                    }
                    }
                }
            }
        }
        Rectangle {
            id: mockDock
            width: Math.min(canvas.width - 160, appDockZone.width + dockZone.width + 20)
            height: Math.max(60, (root.desktop.dockSize || 56) + 16)
            x: root.desktop.dockAlignment === "left" ? (root.desktop.sidebarEdge === "left" ? 76 : 16) : root.desktop.dockAlignment === "right" ? canvas.width - width - (root.desktop.sidebarEdge === "left" ? 16 : 76) : (canvas.width - width) / 2
            y: footer.y - height - 16
            radius: Theme.shellRadius; color: Theme.shellSurface
            opacity: root.desktop.dock === false ? 0.5 : 1
            Behavior on x { NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
            Zone {
                id: appDockZone; zoneName: "dock-apps"; label: "Pinned apps"; items: root.dockApplications
                x: 4; y: 4; width: Math.max(64, Math.min(canvas.width - 340, items.length * ((root.desktop.dockSize || 56) + 4) + 16)); height: parent.height - 8
                chipHeight: height - 8; chipWidth: Math.max(28, Math.min(root.desktop.dockSize || 56, (width - 16) / Math.max(1, items.length) - 4))
            }
            Rectangle { x: appDockZone.x + appDockZone.width + 4; y: 16; width: 1; height: parent.height - 32; color: Theme.outline }
            Zone { id: dockZone; zoneName: "dock"; label: "Dock widgets"; x: appDockZone.x + appDockZone.width + 12; y: 4; width: Math.max(64, root.layout.dock.length * 40 + 16); height: parent.height - 8; contentTop: (height - chipHeight) / 2 }
        }
        Rectangle {
            id: footer
            y: parent.height - height; width: parent.width; height: 56; color: Theme.barBackground
            RowLayout {
                anchors.fill: parent; anchors.margins: 10; spacing: 6
                ActionButton { text: "Dock"; trailingIconName: "pan-down-symbolic"; flat: true; onClicked: root.optionsPage = root.optionsPage === "Dock" ? "" : "Dock" }
                ActionButton { text: "Sidebar"; trailingIconName: "pan-down-symbolic"; flat: true; onClicked: root.optionsPage = root.optionsPage === "Sidebar" ? "" : "Sidebar" }
                ActionButton { visible: !!root.selectedWidget; text: "Move " + (root.info(root.selectedWidget)?.label || "widget"); trailingIconName: "pan-down-symbolic"; flat: true; onClicked: root.optionsPage = root.optionsPage === "Move" ? "" : "Move" }
                Item { Layout.fillWidth: true }
                Text { visible: root.settings.status !== "" && root.settings.status !== "Saved"; text: root.settings.status; color: Theme.warning; Layout.maximumWidth: 260; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                ActionButton { text: "Restore defaults"; flat: true; enabled: !root.settings.busy; onClicked: root.layout = DesktopLayout.defaults() }
                ActionButton { objectName: "customiseCancel"; text: "Cancel"; enabled: !root.settings.busy; onClicked: root.cancel() }
                ActionButton { objectName: "customiseApply"; text: root.settings.busy ? "Saving…" : "Done"; enabled: !root.settings.busy; onClicked: root.apply() }
            }
        }
        Rectangle {
            id: optionsPopover
            visible: root.optionsPage !== ""
            x: 12; y: footer.y - height - 8; width: Math.min(460, canvas.width - 24); height: Math.min(optionContents.implicitHeight + 32, footer.y - 64)
            radius: Theme.cardRadius; color: Theme.shellSurface
            Flickable {
                anchors.fill: parent; anchors.margins: 16; clip: true; contentWidth: width; contentHeight: optionContents.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ColumnLayout {
                    id: optionContents; width: parent.width; spacing: 12
                    RowLayout {
                        Layout.fillWidth: true
                        Text { Layout.fillWidth: true; text: root.optionsPage; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.Medium }
                        IconButton { iconName: "window-close-symbolic"; label: "Close options"; onClicked: root.optionsPage = "" }
                    }
                    Flow {
                        visible: root.optionsPage === "Move"; Layout.fillWidth: true; spacing: 4
                        Repeater {
                            model: [{id: "top-left", label: "Top left"}, {id: "top-center", label: "Top centre"}, {id: "top-right", label: "Top right"}, {id: "dock", label: "Dock"}, {id: "sidebar", label: "Sidebar"}, {id: "control-centre", label: "Control centre"}, {id: "palette", label: "Remove"}]
                            ActionButton { required property var modelData; objectName: "customise-destination-" + modelData.id; text: modelData.label; enabled: root.accepts(root.selectedWidget, modelData.id); onClicked: { root.put(root.selectedWidget, modelData.id, root.layout[modelData.id]?.length || 0); root.optionsPage = ""; } }
                        }
                    }
                        ColumnLayout {
                            visible: root.optionsPage === "Dock"; Layout.fillWidth: true; spacing: 16
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
                            visible: root.optionsPage === "Sidebar"; Layout.fillWidth: true; spacing: 16
                            SettingsHeading { title: "Sidebar"; description: "Drag panel widgets to change their order. Keep at least one panel." }
                            ControlRow { title: "Show sidebar"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.sidebar !== false; onToggleRequested: root.change("sidebar", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Screen edge"; choices: ["Left", "Top", "Right"]; values: ["left", "top", "right"]; value: root.desktop.sidebarEdge || "right"; onChosen: value => root.change("sidebarEdge", value) }
                        }

                }
            }
        }
        Rectangle {
            visible: root.draggedId !== ""; x: root.pointer.x + 12; y: root.pointer.y + 12
            width: dragLabel.implicitWidth + 28; height: 40; radius: 8; color: Theme.elevated
            Text { id: dragLabel; anchors.centerIn: parent; text: root.info(root.draggedId)?.label || ""; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
        }
    }
}
