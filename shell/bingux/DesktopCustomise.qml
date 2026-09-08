import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "DesktopLayout.js" as DesktopLayout

Scope {
    id: root
    readonly property alias preview: canvas
    readonly property alias nativeWindow: editWindow
    readonly property alias contentItem: editWindow.contentItem
    property alias screen: editWindow.screen
    readonly property real width: editWindow.width
    readonly property real height: editWindow.height
    required property var settings
    property bool visible: false
    property bool initialised: false
    // Reuse the render window after the first open so previews retain their scene.
    PanelWindow {
        id: editWindow
        visible: root.initialised
        color: "transparent"
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        exclusiveZone: 0
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "gnoblin-shell-popup"
        WlrLayershell.keyboardFocus: root.visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        mask: Region { width: root.visible ? editWindow.width : 0; height: root.visible ? editWindow.height : 0 }
    }
    property var desktop: ({})
    property var layout: DesktopLayout.defaults()
    property string tab: "Widgets"
    property string appFilter: ""
    property string optionsPage: ""
    property string selectedContainer: "top-right"
    readonly property var containerChoices: [{id: "top-left", label: "Top left"}, {id: "top-center", label: "Top centre"}, {id: "top-right", label: "Top right"}, {id: "dock", label: "Dock"}, {id: "sidebar", label: "Sidebar"}, {id: "control-centre", label: "Control centre"}]
    readonly property var iconChoices: ["system-search-symbolic", "preferences-system-symbolic", "x-office-calendar-symbolic", "preferences-system-notifications-symbolic", "computer-symbolic", "input-keyboard-symbolic", "view-more-symbolic", "microphone-sensitivity-high-symbolic", "media-record-symbolic", "utilities-terminal-symbolic", "accessories-text-editor-symbolic", "applications-multimedia-symbolic", "view-list-symbolic", "network-wireless-symbolic", "bluetooth-active-symbolic", "network-vpn-symbolic", "notifications-disabled-symbolic", "night-light-symbolic", "power-profile-balanced-symbolic", "display-brightness-symbolic", "audio-volume-high-symbolic", "audio-input-microphone-symbolic", "system-lock-screen-symbolic", "avatar-default-symbolic", "starred-symbolic", "user-home-symbolic", "folder-symbolic", "web-browser-symbolic", "mail-unread-symbolic", "camera-photo-symbolic", "view-pin-symbolic", "document-edit-symbolic"]
    property string selectedWidget: ""
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
        if (!id.startsWith("app:")) return DesktopLayout.widget(id);
        const entry = appEntry(id.slice(4));
        return {id, label: entry?.name || id.slice(4), icon: entry?.icon || "application-x-executable", app: true};
    }
    function containerFor(id) { return id.startsWith("app:") ? "dock" : id.startsWith("control-") ? "control-centre" : DesktopLayout.zone(layout, id); }
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
    function accepts(id, target) {
        return id.startsWith("app:") ? ["dock-apps", "dock", "palette"].includes(target) : DesktopLayout.accepts(id, target);
    }
    function open() {
        if (!settings.shellHosted) { settings.requestShellCustomise(); return; }
        desktop = JSON.parse(JSON.stringify(settings.draft.desktop));
        originalDesktop = JSON.parse(JSON.stringify(desktop)); originalDirty = settings.dirty;
        originalChanges = JSON.parse(JSON.stringify(settings.changedSettings));
        desktop.sidebarEdge = desktop.sidebarEdge || settings.currentSidebarEdge;
        layout = JSON.parse(JSON.stringify(desktop.layout || settings.currentLayout || DesktopLayout.defaults()));
        selectedWidget = ""; draggedId = ""; applying = false; tab = "Widgets"; optionsPage = ""; appFilter = "";
        restoreSettings = settings.visible;
        settings.visible = false;
        DesktopEditing.editor = root;
        initialised = true;
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
            if (target !== "palette") pins.splice(Math.max(0, Math.min(index, pins.length)), 0, id.slice(4));
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
    function drag(id, source, x, y) { dragGlobal(id, source.mapToItem(root.preview, x, y)); }
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
        readonly property var info: root.baseInfo(widgetId)
        width: 260; height: 160
        opacity: root.draggedId === widgetId ? 0.35 : 1
        Rectangle { anchors.fill: parent; anchors.margins: 4; radius: 8; color: chipMouse.containsMouse ? Theme.hover : "transparent" }
        Item {
            x: 8; y: 8; width: parent.width - 16; height: 112
            WidgetPreview { anchors.fill: parent; widgetId: chip.widgetId; visible: !chip.info?.app; metrics: root.settings.systemMetrics }
            AppIcon {
                visible: !!chip.info?.app
                anchors.centerIn: parent
                implicitSize: root.desktop.dockSize || 56
                group: root.settings.dockView?.appGroups.find(group => root.appId(group.desktopEntry?.id || group.id) === chip.widgetId.slice(4)) || ({id: chip.widgetId.slice(4), desktopEntry: root.appEntry(chip.widgetId.slice(4))})
                activeStreams: root.settings.dockView?.activity.activeStreams || []
                notifications: root.settings.dockView?.notifications || []
            }
        }
        Text {
            x: 8; y: 124; width: parent.width - 16
            text: chip.info?.label || ""; textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight
            color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        }
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: info?.label || "Widget"
        Keys.onReturnPressed: { root.selectedWidget = widgetId; root.optionsPage = "Widget"; }
        DragHandle { id: chipMouse; widgetId: chip.widgetId; onDoubleClicked: { root.selectedWidget = widgetId; root.optionsPage = "Widget"; } }
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
            x: root.leftInset + 24
            y: Theme.barHeight + 40 + root.topInset; width: Math.max(200, canvas.width - root.rightInset - x - Theme.notificationWidth - 88); height: canvas.height - y - 180
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
                    cellWidth: 260; cellHeight: 160; clip: true; cacheBuffer: 100; reuseItems: true
                    model: !root.visible ? [] : root.tab === "Apps" ? root.applications.map(entry => "app:" + root.appId(entry.id)) : DesktopLayout.widgets.concat(DesktopLayout.controlWidgets).map(widget => widget.id)
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
            x: palette.x; y: palette.y + 70; width: Math.min(420, palette.width); height: Math.min(optionContents.implicitHeight + 32, footer.y - y - 12)
            radius: Theme.cardRadius; color: Qt.rgba(Theme.shellSurface.r, Theme.shellSurface.g, Theme.shellSurface.b, 1)
            Flickable {
                anchors.fill: parent; anchors.margins: 16; clip: true; contentWidth: width; contentHeight: optionContents.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ColumnLayout {
                    id: optionContents; width: parent.width; spacing: 12
                    RowLayout {
                        Layout.fillWidth: true
                        Text { Layout.fillWidth: true; text: root.optionsPage === "Container" ? root.containerChoices.find(item => item.id === root.selectedContainer)?.label || "Container" : root.optionsPage; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.Medium }
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
                        visible: root.optionsPage === "Container"; Layout.fillWidth: true; spacing: 12
                        SettingsChoice { label: "Show"; choices: ["Original", "Icons", "Text", "Icons and text"]; values: ["native", "icons", "text", "both"]; value: root.desktop.containers?.[root.selectedContainer]?.display || "native"; onChosen: value => root.containerDisplay(value) }
                    }
                    ColumnLayout {
                        visible: root.optionsPage === "Widget"; Layout.fillWidth: true; spacing: 12
                        SettingsHeading { title: root.info(root.selectedWidget).label; description: "Use the container style, or change this widget." }
                        SettingsChoice { label: "Show"; choices: ["Follow container", "Original", "Icons", "Text", "Both"]; values: ["inherit", "native", "icons", "text", "both"]; value: root.desktop.widgetOptions?.[root.selectedWidget]?.display || "inherit"; onChosen: value => root.widgetOption("display", value) }
                        SettingsField { objectName: "customiseWidgetLabel"; Layout.margins: 0; label: "Label"; text: root.desktop.widgetOptions?.[root.selectedWidget]?.label || ""; placeholderText: root.baseInfo(root.selectedWidget)?.label || "Widget label"; onEdited: value => root.widgetOption("label", value) }
                        Text { text: "Icon"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                        GridLayout {
                            Layout.fillWidth: true; columns: 8; rowSpacing: 4; columnSpacing: 4
                            Repeater {
                                model: root.iconChoices
                                IconButton { required property string modelData; objectName: "customise-icon-" + modelData; iconName: modelData; label: modelData.replace(/-symbolic$/, "").replace(/-/g, " "); implicitWidth: 36; implicitHeight: 36; onClicked: root.widgetOption("icon", modelData) }
                            }
                        }
                        RowLayout {
                            ActionButton { text: "Reset widget"; flat: true; onClicked: { const next = Object.assign({}, root.desktop.widgetOptions || {}); delete next[root.selectedWidget]; root.change("widgetOptions", next); } }
                            ActionButton { text: "Container style"; flat: true; onClicked: { root.selectedContainer = root.containerFor(root.selectedWidget) || "top-right"; root.optionsPage = "Container"; } }
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
                            SettingsChoice { label: "Click"; choices: ["Focus or minimise", "Always focus", "New window"]; values: ["toggle", "focus", "launch"]; value: root.desktop.dockClick || "toggle"; onChosen: value => root.change("dockClick", value) }
                            SettingsChoice { label: "Middle click"; choices: ["New window", "Close windows", "Do nothing"]; values: ["launch", "close", "none"]; value: root.desktop.dockMiddleClick || "launch"; onChosen: value => root.change("dockMiddleClick", value) }
                            SettingsChoice { label: "Mouse wheel and trackpad"; choices: ["Switch windows", "Do nothing"]; values: ["cycle", "none"]; value: root.desktop.dockScroll || "cycle"; onChosen: value => root.change("dockScroll", value) }
                            SettingsChoice { label: "Scroll direction"; choices: ["Normal", "Reverse"]; values: ["natural", "reverse"]; value: root.desktop.dockScrollDirection || "natural"; onChosen: value => root.change("dockScrollDirection", value) }
                        }
                        ColumnLayout {
                            visible: root.optionsPage === "Sidebar" || (root.optionsPage === "Container" && root.selectedContainer === "sidebar"); Layout.fillWidth: true; spacing: 16

                            ControlRow { title: "Show sidebar"; iconName: ""; toggleVisible: true; toggleChecked: root.desktop.sidebar !== false; onToggleRequested: root.change("sidebar", !toggleChecked); onClicked: toggleRequested() }
                            SettingsChoice { label: "Screen edge"; choices: ["Left", "Top", "Right"]; values: ["left", "top", "right"]; value: root.desktop.sidebarEdge || "right"; onChosen: value => root.change("sidebarEdge", value) }
                        }

                }
            }
        }
        Image {
            visible: root.draggedId !== ""; x: root.pointer.x + 12; y: root.pointer.y + 12; z: 20000
            source: DesktopEditing.previews[root.draggedId]?.url || ""
            width: Math.min(320, DesktopEditing.previews[root.draggedId]?.width || 0)
            height: Math.min(100, DesktopEditing.previews[root.draggedId]?.height || 0)
            fillMode: Image.PreserveAspectFit
        }
    }
}
