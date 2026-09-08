import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

Scope {
    id: root
    readonly property alias preview: canvas
    readonly property alias nativeWindow: editWindow
    readonly property alias optionsWindow: inspectorWindow
    readonly property alias optionsContentItem: inspectorWindow.contentItem
    readonly property alias contentItem: editWindow.contentItem
    property alias screen: editWindow.screen
    readonly property real width: editWindow.width
    readonly property real height: editWindow.height
    required property var settings
    property bool visible: false
    property bool initialised: false
    UiSession {
        id: stackingSession
        sessionName: root.initialised ? "customise" : ""
        state: ({visible: root.visible, surface: "bingux-customise", companionsAbove: true,
            companions: DesktopEditing.surfaces.map(surface => surface.window.WlrLayershell.namespace)
                .concat(["gnoblin-shell-popup", "bingux-sidebar-corner", "bingux-panel-outline"])})
    }
    // Reuse the render window after the first open so previews retain their scene.
    PanelWindow {
        id: editWindow
        visible: root.initialised
        color: "transparent"
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        exclusiveZone: 0
        WlrLayershell.layer: stackingSession.capabilities.includes("ui-sessions") ? WlrLayer.Overlay : WlrLayer.Top
        WlrLayershell.namespace: "bingux-customise"
        WlrLayershell.keyboardFocus: root.visible && root.optionsPage === "" ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        mask: Region { width: root.visible ? editWindow.width : 0; height: root.visible ? editWindow.height : 0 }
    }
    // A pointer-transparent surface keeps the pickup image above native containers.
    PanelWindow {
        id: dragWindow
        // Map after the containers so the pickup stays above their content.
        visible: root.visible && root.draggedId !== ""
        screen: editWindow.screen
        color: "transparent"
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-customise-drag"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        mask: Region { width: 0; height: 0 }
        Image {
            visible: root.visible && root.draggedId !== ""
            source: root.dragImage
            x: root.pointer.x - root.dragHotSpot.x
            y: root.pointer.y - root.dragHotSpot.y
            width: root.dragSize.width; height: root.dragSize.height
            cache: false
        }
    }
    Instantiator {
        model: root.initialised ? DesktopEditing.surfaces.reduce((windows, surface) => windows.includes(surface.window) ? windows : windows.concat([surface.window]), [root.nativeWindow]) : []
        delegate: DropArea {
            required property var modelData
            parent: modelData.contentItem
            anchors.fill: parent
            z: -1
            enabled: DesktopEditing.active
            keys: ["application/x-bingux-widget"]
            function track(event) {
                if (!root.draggedId) return;
                root.pointer = DesktopEditing.point(this, modelData, event.x, event.y);
                root.hoverZone = "";
                event.accepted = true;
            }
            onEntered: drag => track(drag)
            onPositionChanged: drag => track(drag)
        }
    }
    PanelWindow {
        id: inspectorWindow
        visible: root.initialised
        screen: editWindow.screen
        color: "transparent"
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "gnoblin-shell-popup"
        WlrLayershell.keyboardFocus: root.visible && root.optionsPage !== "" ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        mask: Region { x: optionsPopover.x; y: optionsPopover.y; width: root.visible && root.optionsPage ? optionsPopover.width : 0; height: root.visible && root.optionsPage ? optionsPopover.height : 0 }
        contentItem.Keys.onEscapePressed: root.optionsPage = ""
    }
    property var desktop: ({})
    property var layout: DesktopLayout.defaults()
    property var undoStack: []
    property var redoStack: []
    property bool groupingChange: false
    function snapshot() { return JSON.stringify({desktop, layout}); }
    function checkpoint() {
        if (groupingChange) return;
        const state = snapshot();
        if (undoStack[undoStack.length - 1] !== state) undoStack = undoStack.slice(-49).concat([state]);
        redoStack = [];
    }
    function restoreSnapshot(state) {
        const saved = JSON.parse(state);
        desktop = saved.desktop; layout = saved.layout;
        optionsPage = "";
    }
    function undo() {
        if (!undoStack.length || draggedId) return;
        redoStack = redoStack.concat([snapshot()]);
        const state = undoStack[undoStack.length - 1];
        undoStack = undoStack.slice(0, -1); restoreSnapshot(state);
    }
    function redo() {
        if (!redoStack.length || draggedId) return;
        undoStack = undoStack.concat([snapshot()]);
        const state = redoStack[redoStack.length - 1];
        redoStack = redoStack.slice(0, -1); restoreSnapshot(state);
    }
    property string tab: "Widgets"
    property string appFilter: ""
    property bool iconsExpanded: false
    property bool behaviourExpanded: false
    property rect paletteSelection: Qt.rect(0, 0, 0, 0)
    readonly property string inspectedZone: optionsPage === "Container" ? selectedContainer : optionsPage === "Dock" ? "dock" : optionsPage === "Sidebar" ? "sidebar" : containerFor(selectedWidget)
    readonly property rect inspectionAnchor: {
        const surface = DesktopEditing.surfaces.find(area => area.zoneName === inspectedZone && area.visible && area.window.visible);
        if (surface) {
            const item = optionsPage === "Widget" || optionsPage === "Move" ? surface.entries.find(entry => entry.id === selectedWidget)?.item : null;
            if (item && item.visible) {
                const geometry = [item.x, item.y, item.width, item.height, surface.screenRect];
                const point = DesktopEditing.point(item, surface.window, 0, 0);
                return Qt.rect(point.x, point.y, item.width, item.height);
            }
            return surface.screenRect;
        }
        return paletteSelection.width ? paletteSelection : Qt.rect(palette.x, palette.y + 100, 160, 100);
    }
    onOptionsPageChanged: { iconsExpanded = false; behaviourExpanded = false; }
    onDraggedIdChanged: if (draggedId) optionsPage = "";
    property string optionsPage: ""
    property string selectedContainer: "top-right"
    readonly property var containerChoices: [{id: "top-left", label: "Top left"}, {id: "top-center", label: "Top centre"}, {id: "top-right", label: "Top right"}, {id: "dock", label: "Dock"}, {id: "sidebar", label: "Sidebar"}, {id: "control-centre", label: "Control centre"}]
    readonly property var iconChoices: ["system-search-symbolic", "preferences-system-symbolic", "x-office-calendar-symbolic", "preferences-system-notifications-symbolic", "computer-symbolic", "input-keyboard-symbolic", "view-more-symbolic", "microphone-sensitivity-high-symbolic", "media-record-symbolic", "utilities-terminal-symbolic", "accessories-text-editor-symbolic", "applications-multimedia-symbolic", "view-list-symbolic", "network-wireless-symbolic", "bluetooth-active-symbolic", "network-vpn-symbolic", "notifications-disabled-symbolic", "night-light-symbolic", "power-profile-balanced-symbolic", "display-brightness-symbolic", "audio-volume-high-symbolic", "audio-input-microphone-symbolic", "system-lock-screen-symbolic", "avatar-default-symbolic", "starred-symbolic", "user-home-symbolic", "folder-symbolic", "web-browser-symbolic", "mail-unread-symbolic", "camera-photo-symbolic", "view-pin-symbolic", "document-edit-symbolic"]
    property string selectedWidget: ""
    property url dragImage: ""
    property point dragHotSpot: Qt.point(0, 0)
    property size dragSize: Qt.size(0, 0)
    property string draggedId: ""
    property string hoverZone: ""
    property int hoverIndex: 0
    property point pointer: Qt.point(0, 0)
    property string wallpaper: ""
    property bool restoreSettings: false
    onVisibleChanged: if (!visible && restoreSettings) { restoreSettings = false; settings.visible = true; }
    property bool applying: false
    property var originalDesktop: ({})
    property bool originalDirty: false
    property var originalChanges: ({})
    function cancel() {
        if (applying) { settings.draft = Object.assign({}, settings.draft, {desktop: originalDesktop}); settings.dirty = originalDirty; settings.changedSettings = originalChanges; }
        applying = false; visible = false;
    }
    readonly property real topInset: settings.currentSidebarEdge === "top" ? (DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar" && surface.window.visible)?.screenRect.height || 0) : 0
    readonly property real leftInset: settings.currentSidebarEdge === "left" ? sidebarExtent : 0
    readonly property real rightInset: settings.currentSidebarEdge === "right" ? sidebarExtent : 0
    readonly property real sidebarExtent: {
        const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar" && surface.window.visible);
        return area ? area.screenRect.width : 0;
    }
    readonly property var zones: DesktopEditing.surfaces
    function selectContainer(id) {
        selectedContainer = id; selectedWidget = "";
        optionsPage = "Container";
    }
    function appId(value) { return value.endsWith(".desktop") ? value.slice(0, -8) : value; }
    function appEntry(value) { return DesktopEntries.byId(value) || DesktopEntries.byId(value + ".desktop") || DesktopEntries.heuristicLookup(value); }
    function baseInfo(id) {
        if (!id.startsWith("app:")) return ControlLayout.widget(id) || DesktopLayout.widget(id);
        const entry = appEntry(id.slice(4));
        return {id, label: entry?.name || id.slice(4), icon: entry?.icon || "application-x-executable", app: true};
    }
    function containerFor(id) { if (ControlLayout.groupFor(id)) return "control-centre"; return id.startsWith("app:") ? "dock" : DesktopLayout.zone(layout, id) || (id.startsWith("control-") ? "control-centre" : ""); }
    function appearance(id) {
        const item = baseInfo(id);
        return DesktopLayout.presentation(desktop, id, containerFor(id), item?.label || "", item?.icon || "", id !== "clock", id === "clock");
    }
    function info(id) { return Object.assign({}, baseInfo(id), appearance(id)); }
    function widgetOption(key, value) {
        const options = Object.assign({}, desktop.widgetOptions || {});
        options[selectedWidget] = Object.assign({}, options[selectedWidget] || {}, {[key]: value});
        change("widgetOptions", options);
    }
    function containerDisplay(value) {
        change("containers", Object.assign({}, desktop.containers || {}, {[selectedContainer]: {display: value}}));
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
    function orderFor(zone, id) {
        const group = ControlLayout.groupFor(id);
        if (zone === "control-centre") return group ? ControlLayout.items(desktop.controlLayout, group)
            : (desktop.controlOrder || DesktopLayout.controlOrder()).map(name => "control-" + name);
        return zone === "dock" && id.startsWith("app:") ? dockApplications : layout[zone] || [];
    }
    function accepts(id, target) {
        if (ControlLayout.groupFor(id)) return ["control-centre", "palette"].includes(target);
        return id.startsWith("app:") ? ["dock-apps", "dock", "palette"].includes(target) : DesktopLayout.accepts(id, target);
    }
    function open() {
        if (!settings.shellHosted) { settings.requestShellCustomise(); return; }
        desktop = JSON.parse(JSON.stringify(settings.draft.desktop));
        originalDesktop = JSON.parse(JSON.stringify(desktop)); originalDirty = settings.dirty;
        originalChanges = JSON.parse(JSON.stringify(settings.changedSettings));
        desktop.sidebarEdge = desktop.sidebarEdge || settings.currentSidebarEdge;
        layout = JSON.parse(JSON.stringify(desktop.layout || settings.currentLayout || DesktopLayout.defaults()));
        undoStack = []; redoStack = [];
        selectedWidget = ""; draggedId = ""; applying = false; tab = "Widgets"; optionsPage = ""; appFilter = "";
        restoreSettings = settings.visible;
        settings.visible = false;
        DesktopEditing.editor = root;
        initialised = true;
        visible = true;
        wallpaperReader.running = true;
        canvas.forceActiveFocus();
    }
    function change(key, value) { if (JSON.stringify(desktop[key]) === JSON.stringify(value)) return; checkpoint(); desktop = Object.assign({}, desktop, {[key]: value}); }
    function put(id, target, index) {
        if (!accepts(id, target)) return;
        checkpoint(); groupingChange = true;
        try { putItem(id, target, index); } finally { groupingChange = false; }
    }
    function putItem(id, target, index) {
        const group = ControlLayout.groupFor(id);
        if (group) {
            let next = ControlLayout.move(desktop.controlLayout, group, id, target === "palette" ? -1 : index);
            if (target !== "palette" && group !== "control-centre" && !ControlLayout.contains(next, "control-centre", group))
                next = ControlLayout.move(next, "control-centre", group, ControlLayout.position(null, "control-centre", group));
            change("controlLayout", next);
            return;
        }
        if (id.startsWith("control-")) {
            if (!accepts(id, target)) return;
            const name = id.slice(8);
            const order = (desktop.controlOrder || DesktopLayout.controlOrder()).filter(value => value !== name);
            if (target === "control-centre") order.splice(Math.max(0, Math.min(index, order.length)), 0, name);
            change("controlOrder", order);
            layout = DesktopLayout.move(layout, id, target === "control-centre" ? "palette" : target, index);
            if (target === "dock") change("dock", true);
            if (!["network", "bluetooth"].includes(name)) change("controlCentre", Object.assign({vpn: true, dnd: true, nightLight: false, power: false, awake: false}, desktop.controlCentre || {}, {[name]: target !== "palette"}));
            return;
        }
        if (id.startsWith("app:")) {
            if (!accepts(id, target)) return;
            const pins = dockApplications.map(value => value.slice(4)).filter(value => value !== id.slice(4));
            if (target !== "palette") pins.splice(Math.max(0, Math.min(index, pins.length)), 0, id.slice(4));
            const oldOrder = desktop.dockApps?.order || [];
            change("dockApps", {pinnedApps: pins, order: pins.concat(oldOrder.filter(value => !pins.includes(appId(appEntry(value)?.id || value))))});
            if (target !== "palette") change("dock", true);
            return;
        }
        const previous = layout;
        layout = DesktopLayout.move(layout, id, target, index);
        if (layout === previous) return;
        if (DesktopLayout.isTemplate(id)) {
            const created = layout[target]?.find(value => !previous[target].includes(value));
            const options = Object.assign({}, desktop.widgetOptions || {});
            if (created) {
                delete options[created];
                if (options[id]) options[created] = Object.assign({}, options[id]);
                change("widgetOptions", options);
            }
        } else if (target === "palette" && (DesktopLayout.isSpacing(id) || DesktopLayout.isDecoration(id))) {
            const options = Object.assign({}, desktop.widgetOptions || {});
            delete options[id];
            change("widgetOptions", options);
        }
        if (target === "dock") change("dock", true);
        if (id === "metrics" && target !== "palette") change("metrics", true);
        if (target === "sidebar") change("sidebar", true);
    }
    function dragGlobal(id, point) {
        draggedId = id; pointer = point; hoverZone = "";
        for (const area of DesktopEditing.surfaces) {
            if (!area.visible || !area.window.visible) continue;
            const rect = area.screenRect;
            if (point.x < rect.x || point.y < rect.y || point.x > rect.x + rect.width || point.y > rect.y + rect.height) continue;
            if (!accepts(id, area.zoneName)) continue;
            hoverZone = area.zoneName;
            hoverIndex = area.insertionIndex(point, id);
            return;
        }
        const palette = root.preview.paletteRect;
        if (point.x >= palette.x && point.x <= palette.x + palette.width && point.y >= palette.y && point.y <= palette.y + palette.height) {
            hoverZone = "palette"; hoverIndex = 0;
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
            required property Item dragVisual
            anchors.fill: parent; hoverEnabled: true
            preventStealing: true
            cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
            property point start
            property bool hadDrag: false
            onPressed: mouse => { hadDrag = false; start = Qt.point(mouse.x, mouse.y); root.selectedWidget = widgetId; }
            onPositionChanged: mouse => {
                if (pressed && !hadDrag && Math.abs(mouse.x - start.x) + Math.abs(mouse.y - start.y) > 6) {
                    hadDrag = true;
                    nativeDrag.begin(widgetId, dragVisual, root.nativeWindow, DesktopEditing.point(this, root.nativeWindow, start.x, start.y));
                }
            }
            onReleased: nativeDrag.cancelPending()
            onCanceled: nativeDrag.cancelPending()
            WidgetDrag { id: nativeDrag }
        }
    component Chip: Item {
        id: chip
        objectName: "customise-widget-" + widgetId
        required property string widgetId
        readonly property var info: root.baseInfo(widgetId)
        width: paletteGrid.cellWidth; height: paletteGrid.cellHeight
        opacity: root.draggedId === widgetId ? 0.35 : 1
        Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 8; color: chipMouse.containsMouse ? Theme.hover : "transparent" }
        Item {
            x: 8; y: 8; width: parent.width - 16; height: parent.height - 44
            WidgetPreview { id: widgetPreview; anchors.fill: parent; widgetId: chip.widgetId; visible: !chip.info?.app; metrics: root.settings.systemMetrics }
            AppIcon {
                id: appPreview
                visible: !!chip.info?.app
                anchors.centerIn: parent
                implicitSize: root.desktop.dockSize || 56
                group: ({id: chip.widgetId.slice(4), desktopEntry: root.appEntry(chip.widgetId.slice(4))})
                activeStreams: []
                notifications: []
            }
        }
        Text {
            x: 8; y: parent.height - 28; width: parent.width - 16
            text: chip.info?.label || ""; textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight
            color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: info?.label || "Widget"
        function inspect() {
            const point = DesktopEditing.point(chip, root.nativeWindow, 0, 0);
            root.paletteSelection = Qt.rect(point.x, point.y, width, height);
            root.selectedWidget = widgetId; root.optionsPage = "Widget";
        }
        Keys.onReturnPressed: inspect()
        DragHandle { id: chipMouse; widgetId: chip.widgetId; dragVisual: chip.info?.app ? appPreview : widgetPreview.visualItem; onDoubleClicked: chip.inspect() }
    }
    Item {
        id: canvas
        parent: editWindow.contentItem
        visible: root.visible
        readonly property rect paletteRect: Qt.rect(palette.x, palette.y, palette.width, palette.height)
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: {
            if (root.draggedId) { root.draggedId = ""; root.hoverZone = ""; }
            else if (root.optionsPage) root.optionsPage = "";
            else if (!root.settings.busy) root.cancel();
        }
        Image { anchors.fill: parent; source: root.wallpaper; fillMode: Image.PreserveAspectCrop; asynchronous: true }
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.48) }
        Item {
            id: palette
            objectName: "customisePalette"
            readonly property string zoneName: "palette"
            WidgetDropArea { anchors.fill: parent; zoneName: "palette"; window: root.nativeWindow }
            x: root.leftInset + 24
            y: Theme.barHeight + 40 + root.topInset; width: Math.max(200, canvas.width - root.rightInset - x - Theme.notificationWidth - 88); height: canvas.height - y - 180
            Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.width: root.hoverZone === "palette" ? 1 : 0; border.color: Theme.accent }
            ColumnLayout {
                anchors.fill: parent; spacing: 14
                Text { Layout.fillWidth: true; text: root.draggedId ? "Drop into a highlighted container, or here to remove." : "Drag items into your desktop."; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize; font.weight: Font.DemiBold }
                RowLayout {
                    spacing: 4
                    Repeater { model: ["Widgets", "Apps"]; ActionButton { required property string modelData; text: modelData; flat: root.tab !== modelData; onClicked: { root.tab = modelData; root.appFilter = ""; root.optionsPage = ""; } } }
                    Item { Layout.fillWidth: true }
                }
                SettingsField { objectName: "customiseFilter"; label: root.tab === "Apps" ? "Find an app" : "Find a widget"; placeholderText: root.tab === "Apps" ? "Search installed apps" : "Search widgets"; Layout.margins: 0; text: root.appFilter; onEdited: value => root.appFilter = value }
                GridView {
                    id: paletteGrid
                    objectName: "customiseWidgetGrid"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    cellWidth: width / Math.max(1, Math.floor(width / 190)); cellHeight: 144; clip: true; cacheBuffer: 100; reuseItems: true
                    model: !root.visible ? [] : root.tab === "Apps" ? root.applications.map(entry => "app:" + root.appId(entry.id)) : DesktopLayout.layoutWidgets.concat(DesktopLayout.decorationWidgets, DesktopLayout.widgets, DesktopLayout.controlWidgets, ControlLayout.widgets).filter(widget => !root.appFilter || widget.label.toLowerCase().includes(root.appFilter.toLowerCase())).map(widget => widget.id)
                    delegate: Chip { required property string modelData; widgetId: modelData }
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                }
            }
        }
        Rectangle {
            id: footer
            x: root.leftInset; y: parent.height - height; width: parent.width - root.leftInset - root.rightInset; height: 56; color: Theme.barBackground
            RowLayout {
                anchors.fill: parent; anchors.margins: 10; spacing: 6
                IconButton { objectName: "customiseUndo"; iconName: "edit-undo-symbolic"; label: "Undo"; enabled: root.undoStack.length > 0 && !root.draggedId; onClicked: root.undo() }
                IconButton { objectName: "customiseRedo"; iconName: "edit-redo-symbolic"; label: "Redo"; enabled: root.redoStack.length > 0 && !root.draggedId; onClicked: root.redo() }
                Item { Layout.fillWidth: true }
                Text { visible: root.settings.status !== "" && root.settings.status !== "Saved"; text: root.settings.status; color: Theme.warning; Layout.maximumWidth: 260; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                ActionButton { text: "Restore defaults"; flat: true; enabled: !root.settings.busy; onClicked: { root.checkpoint(); root.layout = DesktopLayout.defaults(); } }
                ActionButton { objectName: "customiseCancel"; text: "Cancel"; enabled: !root.settings.busy; onClicked: root.cancel() }
                ActionButton { objectName: "customiseApply"; text: root.settings.busy ? "Saving…" : "Done"; enabled: !root.settings.busy; onClicked: root.apply() }
            }
        }
        MouseArea {
            anchors.fill: parent
            visible: root.optionsPage !== ""
            onPressed: mouse => { root.optionsPage = ""; mouse.accepted = false; }
        }
        Rectangle {
            id: optionsPopover
            objectName: "customiseInspector"
            parent: inspectorWindow.contentItem
            visible: root.visible && root.optionsPage !== ""
            readonly property rect target: root.inspectionAnchor
            readonly property bool beside: ["sidebar", "control-centre"].includes(root.inspectedZone)
            readonly property real desiredX: beside ? (target.x > canvas.width / 2 ? target.x - width - 12 : target.x + target.width + 12) : target.x + target.width / 2 - width / 2
            readonly property real desiredY: root.inspectedZone === "dock" ? target.y - height - 12 : beside ? target.y : target.y + target.height + 12
            x: Math.max(root.leftInset + 12, Math.min(desiredX, canvas.width - root.rightInset - width - 12))
            y: Math.max(Theme.barHeight + root.topInset + 12, Math.min(desiredY, footer.y - height - 12))
            width: Math.min(380, canvas.width - root.leftInset - root.rightInset - 24)
            height: Math.min(optionContents.implicitHeight + 32, footer.y - Theme.barHeight - root.topInset - 36)
            border.width: 1; border.color: Theme.outline
            MouseArea { anchors.fill: parent }
            radius: Theme.cardRadius; color: Qt.rgba(Theme.popupSurface.r, Theme.popupSurface.g, Theme.popupSurface.b, 1)
            Flickable {
                anchors.fill: parent; anchors.margins: 16; clip: true; contentWidth: width; contentHeight: optionContents.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ColumnLayout {
                    id: optionContents; width: parent.width; spacing: 12
                    RowLayout {
                        Layout.fillWidth: true
                        Text { Layout.fillWidth: true; text: root.optionsPage === "Container" ? root.containerChoices.find(item => item.id === root.selectedContainer)?.label || "Container" : root.optionsPage === "Widget" ? root.baseInfo(root.selectedWidget)?.label || "Widget" : root.optionsPage; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.Medium }
                        IconButton { iconName: "window-close-symbolic"; label: "Close options"; onClicked: root.optionsPage = "" }
                    }
                    Flow {
                        visible: root.optionsPage === "Move"; Layout.fillWidth: true; spacing: 4
                        Repeater {
                            model: [{id: "top-left", label: "Top left"}, {id: "top-center", label: "Top centre"}, {id: "top-right", label: "Top right"}, {id: "dock", label: "Dock"}, {id: "sidebar", label: "Sidebar"}, {id: "control-centre", label: "Control centre"}, {id: "palette", label: "Remove"}]
                            ActionButton { required property var modelData; objectName: "customise-destination-" + modelData.id; text: modelData.label; enabled: root.accepts(root.selectedWidget, modelData.id); onClicked: { root.put(root.selectedWidget, modelData.id, root.orderFor(modelData.id, root.selectedWidget).length); root.optionsPage = ""; } }
                        }
                    }
                    ColumnLayout {
                        visible: root.optionsPage === "Container"; Layout.fillWidth: true; spacing: 12
                        SettingsChoice { label: "Show"; choices: ["Original", "Icons", "Text", "Icons and text"]; values: ["native", "icons", "text", "both"]; value: root.desktop.containers?.[root.selectedContainer]?.display || "native"; onChosen: value => root.containerDisplay(value) }
                    }
                    ColumnLayout {
                        visible: root.optionsPage === "Widget"; Layout.fillWidth: true; spacing: 12
                        Text { visible: DesktopLayout.isSpacing(root.selectedWidget); Layout.fillWidth: true; wrapMode: Text.Wrap; text: root.selectedWidget.startsWith("spring") ? "Expands to fill the available room in this part of the bar." : "Leaves a fixed gap between neighbouring items."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        RowLayout {
                            visible: root.selectedWidget.startsWith("spacer:")
                            Text { text: "Width"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            SeekSlider { Layout.fillWidth: true; from: 8; to: 160; stepSize: 4; value: root.desktop.widgetOptions?.[root.selectedWidget]?.width || 20; onMoved: root.widgetOption("width", Math.round(value)); Accessible.name: "Space width" }
                            Text { text: (root.desktop.widgetOptions?.[root.selectedWidget]?.width || 20) + " px"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        }
                        SettingsChoice { visible: !DesktopLayout.isSpacing(root.selectedWidget) && !ControlLayout.groupFor(root.selectedWidget); label: "Show"; choices: ["Follow container", "Original", "Icons", "Text", "Both"]; values: ["inherit", "native", "icons", "text", "both"]; value: root.desktop.widgetOptions?.[root.selectedWidget]?.display || "inherit"; onChosen: value => root.widgetOption("display", value) }
                        SettingsField { visible: !DesktopLayout.isSpacing(root.selectedWidget) && !ControlLayout.groupFor(root.selectedWidget); objectName: "customiseWidgetLabel"; Layout.margins: 0; label: "Label"; text: root.desktop.widgetOptions?.[root.selectedWidget]?.label || ""; placeholderText: root.baseInfo(root.selectedWidget)?.label || "Widget label"; onEdited: value => root.widgetOption("label", value) }
                        ActionButton { objectName: "customiseIconPickerToggle"; visible: !DesktopLayout.isSpacing(root.selectedWidget) && !ControlLayout.groupFor(root.selectedWidget); text: root.iconsExpanded ? "Hide icons" : "Change icon…"; flat: true; onClicked: root.iconsExpanded = !root.iconsExpanded }
                        GridLayout {
                            visible: root.iconsExpanded && !DesktopLayout.isSpacing(root.selectedWidget) && !ControlLayout.groupFor(root.selectedWidget)
                            Layout.fillWidth: true; columns: 8; rowSpacing: 4; columnSpacing: 4
                            Repeater {
                                model: root.iconChoices
                                IconButton { required property string modelData; objectName: "customise-icon-" + modelData; iconName: modelData; label: modelData.replace(/-symbolic$/, "").replace(/-/g, " "); implicitWidth: 36; implicitHeight: 36; onClicked: root.widgetOption("icon", modelData) }
                            }
                        }
                        RowLayout {
                            ActionButton { text: "Move…"; flat: true; onClicked: root.optionsPage = "Move" }
                            ActionButton { text: "Remove"; flat: true; onClicked: { root.put(root.selectedWidget, "palette", 0); root.optionsPage = ""; } }
                            ActionButton { text: "Reset"; flat: true; onClicked: { const next = Object.assign({}, root.desktop.widgetOptions || {}); delete next[root.selectedWidget]; root.change("widgetOptions", next); } }
                        }
                    }
                        ColumnLayout {
                            visible: root.optionsPage === "Dock" || (root.optionsPage === "Container" && root.selectedContainer === "dock"); Layout.fillWidth: true; spacing: 16

                            ControlRow { title: "Show dock"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.dock !== false; onToggleRequested: root.change("dock", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Alignment"; choices: ["Left", "Centre", "Right"]; values: ["left", "center", "right"]; value: root.desktop.dockAlignment || "center"; onChosen: value => root.change("dockAlignment", value) }
                            RowLayout {
                                Text { text: "Icon size"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                SeekSlider { Layout.fillWidth: true; from: 32; to: 80; stepSize: 4; value: root.desktop.dockSize || 56; onMoved: root.change("dockSize", Math.round(value)); Accessible.name: "Dock icon size" }
                                Text { text: (root.desktop.dockSize || 56) + " px"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                            }
                            ActionButton { text: root.behaviourExpanded ? "Hide mouse and trackpad settings" : "Mouse and trackpad…"; flat: true; onClicked: root.behaviourExpanded = !root.behaviourExpanded }
                            SettingsChoice { visible: root.behaviourExpanded; label: "Click"; choices: ["Focus or minimise", "Always focus", "New window"]; values: ["toggle", "focus", "launch"]; value: root.desktop.dockClick || "toggle"; onChosen: value => root.change("dockClick", value) }
                            SettingsChoice { visible: root.behaviourExpanded; label: "Middle click"; choices: ["New window", "Close windows", "Do nothing"]; values: ["launch", "close", "none"]; value: root.desktop.dockMiddleClick || "launch"; onChosen: value => root.change("dockMiddleClick", value) }
                            SettingsChoice { visible: root.behaviourExpanded; label: "Mouse wheel and trackpad"; choices: ["Switch windows", "Do nothing"]; values: ["cycle", "none"]; value: root.desktop.dockScroll || "cycle"; onChosen: value => root.change("dockScroll", value) }
                            SettingsChoice { visible: root.behaviourExpanded; label: "Scroll direction"; choices: ["Normal", "Reverse"]; values: ["natural", "reverse"]; value: root.desktop.dockScrollDirection || "natural"; onChosen: value => root.change("dockScrollDirection", value) }
                        }
                        ColumnLayout {
                            visible: root.optionsPage === "Sidebar" || (root.optionsPage === "Container" && root.selectedContainer === "sidebar"); Layout.fillWidth: true; spacing: 16

                            ControlRow { title: "Show sidebar"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.sidebar !== false; onToggleRequested: root.change("sidebar", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Screen edge"; choices: ["Left", "Top", "Right"]; values: ["left", "top", "right"]; value: root.desktop.sidebarEdge || "right"; onChosen: value => root.change("sidebarEdge", value) }
                        }

                }
            }
        }
    }
}
