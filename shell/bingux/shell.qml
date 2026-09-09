//@ pragma UseQApplication

import QtQuick
import QtCore
import QtQml.Models
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets
import "DesktopLayout.js" as DesktopLayout

ShellRoot {
    id: root

    Connections {
        target: Quickshell
        // The success banner owns another QML engine and blocks IPC until it
        // closes. Its animation can stall behind retained editor surfaces.
        // Keep reload errors visible; successful reloads need no extra window.
        function onReloadCompleted() { Quickshell.inhibitReloadPopup(); }
    }

    readonly property var commandNotifications: notificationState
    property var currentTime: new Date()

    function layoutSnapshot() {
        return {version: 1, layout: topBar.snapshotLayout(),
            desktop: BinguxPreferences.data.desktop, dock: dock.snapshotLayout(),
            controlCentre: ControlCentreServices.effectiveControls, controlLayout: controlCentre.snapshotLayout(),
            sidebar: {edge: terminalSidebar.edge, opened: terminalSidebar.opened,
                contentType: terminalSidebar.contentType},
            controlCentreReady: ControlCentreServices.preferencesReady};
    }
    Timer {
        interval: 200
        running: Quickshell.env("BINGUX_LAYOUT_IMPORT") !== "0" && BinguxPreferences.loaded &&
            !BinguxPreferences.layoutError && (!BinguxPreferences.data.desktop.layoutVersion || !BinguxPreferences.data.desktop.controlLayout) &&
            ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        onTriggered: BinguxPreferences.importDesktop(root.layoutSnapshot())
    }

    function closePanelsExcept(panel) {
        for (const widget of terminalSidebar.panelWidgets) {
            if (panel !== widget.popup) widget.popup.visible = false;
        }
        if (panel !== windowSwitcher && windowSwitcher.active) windowSwitcher.close();
        if (panel !== searchOverlay && searchOverlay.visible) searchOverlay.closeSearch();
        if (panel !== emojiPicker) emojiPicker.visible = false;
        if (panel !== calendarPopup) calendarPopup.visible = false;
        if (panel !== controlCentre && panel?.anchorWindow !== controlCentre.nativeWindow) controlCentre.visible = false;
        if (panel !== metricsPopup) metricsPopup.visible = false;
        if (panel !== barOverflow) barOverflow.visible = false;
        if (panel !== notificationCentre) notificationCentre.visible = false;
        if (panel !== inputSourceSelector) inputSourceSelector.menuOpen = false;
        if (panel !== captureTool && captureTool.opened) captureTool.close();
    }

    function openSearch() {
        searchOverlay.showSearch();
    }
    function openWidgetMenu(id, item, window) {
        widgetMenu.widgetId = id;
        widgetMenu.anchorItem = item;
        widgetMenu.anchorWindow = window.nativeWindow || window;
        widgetMenu.visible = true;
    }
    TrayMenu {
        id: widgetMenu
        property string widgetId: ""
        readonly property bool application: widgetId.startsWith("app:")
        readonly property var appGroup: application ? dock.appGroups.find(group =>
            "app:" + dock.pinIdentity(group.desktopEntry?.id || group.id) === widgetId) : null
        screen: topBar.screen
        actions: (application ? [
            {text: appGroup && dock.isPinned(appGroup) ? "Unpin from dock" : "Pin to dock",
                enabled: !!appGroup && (dock.isPinned(appGroup) || !!appGroup.desktopEntry),
                icon: appGroup && dock.isPinned(appGroup) ? "list-remove-symbolic" : "view-pin-symbolic",
                triggered: () => { if (appGroup) dock.setPinned(appGroup, !dock.isPinned(appGroup)); }}
        ] : []).concat([
            {text: "Customise…", enabled: true, icon: "preferences-system-symbolic", triggered: () => binguxSettings.openCustomise(widgetId, "Widget")},
            {text: "Move…", enabled: true, icon: "transform-move-symbolic", triggered: () => binguxSettings.openCustomise(widgetId, "Move")}
        ], application ? [] : [
            {text: "Remove", enabled: true, icon: "list-remove-symbolic", triggered: () => binguxSettings.openCustomise(widgetId, "Remove")}
        ]).map(action => Object.assign({isSeparator: false, hasChildren: false, checkState: Qt.Unchecked}, action))
    }

    Connections {
        target: DesktopEditing
        function onActiveChanged() {
            if (DesktopEditing.active) {
                root.closePanelsExcept(null);
                widgetMenu.visible = false;
                controlCentre.detailOpen = false;
                Qt.callLater(() => controlCentre.visible = DesktopEditing.active);
            } else controlCentre.visible = false;
        }
    }
    ProfileSettings {
        id: profileSettings
    }

    TerminalSidebar {
        id: terminalSidebar
        onPanelOpening: popup => root.closePanelsExcept(popup)
        onWidgetEditRequested: (id, item, window) => root.openWidgetMenu(id, item, window)
        onCustomiseRequested: {
            if (DesktopEditing.active) desktopCustomiser.selectContainer("sidebar");
            else binguxSettings.openCustomise("", "Sidebar");
        }
        systemMetrics: metrics
        widgetLayout: topBar
        settings: profileSettings
        screen: topBar.screen
        inputSuspended: captureTool.opened
    }

    // Paint below the bar without increasing its window or exclusive zone.
    PanelWindow {
        screen: topBar.screen
        visible: topBar.visible && terminalSidebar.topInset === 0
        implicitHeight: 1
        color: Theme.panelOuterOutline
        exclusionMode: ExclusionMode.Ignore
        mask: Region {}
        anchors { top: true; left: true; right: true }
        margins.top: topBar.margins.top + Theme.barHeight
        margins.left: topBar.margins.left + (terminalSidebar.leftInset > 0 ? terminalSidebar.desktopCornerSize : 0)
        margins.right: topBar.margins.right + (terminalSidebar.rightInset > 0 ? terminalSidebar.desktopCornerSize : 0)
        WlrLayershell.layer: topBar.WlrLayershell.layer
        WlrLayershell.namespace: "bingux-panel-outline"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    }

    Metrics {
        id: metrics
    }

    UiSession {
        id: popouts
        sessionName: "desktop"
        state: {
            const groups = {};
            for (const entry of notificationState.allEntries) {
                const key = JSON.stringify([entry.desktopEntry || "", entry.appName || ""]);
                if (!groups[key]) groups[key] = {desktopEntry: entry.desktopEntry || "", appName: entry.appName || "", count: 0};
                groups[key].count++;
            }
            return {pinnedApps: dock.pinnedApps, activeStreams: dock.activity.activeStreams,
                notificationGroups: Object.values(groups)};
        }
        onCommandReceived: command => {
            if (command.action === "settings") binguxSettings.visible = true;
            else if (command.action === "pin" && typeof command.id === "string") {
                const entry = dock.desktopEntryFor(dock.normaliseAppId(command.id));
                if (entry) dock.setPinned({id: entry.id, desktopEntry: entry, windows: []}, command.pinned === true);
            }
        }
    }
    QtObject {
        id: searchOverlay
        readonly property bool visible: popouts.states.search?.visible || false
        onVisibleChanged: if (visible) root.closePanelsExcept(searchOverlay)
        function showSearch() { popouts.command("search", {action: "open"}); }
        function closeSearch() { popouts.command("search", {action: "hide"}); }
    }
    QtObject {
        id: windowSwitcher
        readonly property bool active: popouts.states.switcher?.visible || false
        onActiveChanged: if (active) { root.closePanelsExcept(windowSwitcher); dock.closeMenus(); }
        function close() { popouts.command("switcher", {action: "close"}); }
    }

    PrivacyState { id: privacySession }

    CaptureTool {
        id: captureTool
        screen: topBar.screen
        onOpening: { root.closePanelsExcept(captureTool); dock.closeMenus(); }
    }

    EmojiPicker {
        id: emojiPicker
        screen: topBar.screen
        onOpening: { root.closePanelsExcept(emojiPicker); dock.closeMenus(); }
    }

    NotificationState {
        doNotDisturb: ControlCentreServices.doNotDisturb
        id: notificationState
    }

    NotificationSurface {
        id: notificationSurface
        sidebarScreen: terminalSidebar.screen
        leftInset: terminalSidebar.leftInset
        rightInset: terminalSidebar.rightInset
        inputSuspended: captureTool.opened || DesktopEditing.active
        notificationCentre: notificationCentre
        screen: topBar.screen
        state: notificationState
    }

    OsdState {
        id: osdState
    }

    OsdSurface {
        state: osdState
        dockSafeInset: dock.dockTopFromBottom + Theme.dockPadding
        sidebarScreen: terminalSidebar.screen
        leftInset: terminalSidebar.leftInset
        rightInset: terminalSidebar.rightInset
    }

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.currentTime = new Date()
    }

    ShellCommands { inputSelector: inputSourceSelector; indicators: systemIndicators; mediaControls: controlCentre; notificationState: root.commandNotifications; dockView: dock }

    DesktopCustomise { id: desktopCustomiser; settings: binguxSettings; screen: topBar.screen; onSidebarPanelRequested: id => terminalSidebar.selectContent(id) }
    BinguxSettings { id: binguxSettings; customiser: desktopCustomiser; shellHosted: true; systemMetrics: metrics; dockView: dock; currentLayout: topBar.snapshotLayout(); currentSidebarEdge: terminalSidebar.edge; onVisibleChanged: if (visible) root.closePanelsExcept(null) }

    IpcHandler {
        target: "shell"
        function customise(): void { binguxSettings.openCustomise("", ""); }
        function customiseContainer(container: string): void { binguxSettings.openContainerCustomise(container); }
        function status(): string {
            return JSON.stringify({search: searchOverlay.visible, calendar: calendarPopup.visible,
                controls: controlCentre.visible, notifications: notificationCentre.visible,
                metrics: metricsPopup.visible, keyboard: inputSourceSelector.menuOpen,
                capture: captureTool.state, sidebar: terminalSidebar.opened,
                doNotDisturb: ControlCentreServices.doNotDisturb});
        }
        function reload(): void { Qt.callLater(() => Quickshell.reload(false)); }
        function layoutSnapshot(): string {
            return JSON.stringify(root.layoutSnapshot());
        }
        function panel(name: string, action: string): string {
            const panels = {settings: binguxSettings, calendar: calendarPopup, controls: controlCentre,
                notifications: notificationCentre, metrics: metricsPopup, search: searchOverlay};
            if (!(name in panels) && name !== "keyboard")
                return JSON.stringify({ok: false, error: "Unknown panel: " + name});
            if (!["open", "close", "toggle", "status"].includes(action))
                return JSON.stringify({ok: false, error: "Unknown panel action: " + action});
            const current = name === "keyboard" ? inputSourceSelector.menuOpen : panels[name].visible;
            if (action === "status") return JSON.stringify({open: current});
            const show = action === "toggle" ? !current : action === "open";
            if (name === "search") {
                if (show) root.openSearch(); else searchOverlay.closeSearch();
            } else if (name === "keyboard") {
                if (show) inputSourceSelector.openMenu(); else inputSourceSelector.menuOpen = false;
            } else panels[name].visible = show;
            return JSON.stringify({ok: true, open: show});
        }
        function dnd(action: string): string {
            if (!["on", "off", "toggle", "status"].includes(action))
                return JSON.stringify({ok: false, error: "Unknown Do Not Disturb action"});
            if (action === "status") return JSON.stringify({enabled: ControlCentreServices.doNotDisturb});
            if (!ControlCentreServices.ready || ControlCentreServices.busy || !ControlCentreServices.state.dndAvailable)
                return JSON.stringify({ok: false, error: "Do Not Disturb is unavailable or busy"});
            const enabled = action === "toggle" ? !ControlCentreServices.doNotDisturb : action === "on";
            ControlCentreServices.action({kind: "dnd", enabled});
            return JSON.stringify({ok: true, requested: enabled});
        }
        function calendar(): void { calendarPopup.visible = !calendarPopup.visible }
        function search(): void { root.openSearch() }
        function emoji(): void { emojiPicker.open() }
        function notificationStatus(): string { return JSON.stringify({count: notificationState.allEntries.length, ready: notificationState.historyReady, open: notificationCentre.visible, retained: notificationCentre.retained, surface: notificationSurface.visible, suspended: notificationSurface.inputSuspended, x: notificationSurface.viewport.x, y: notificationSurface.viewport.y, width: notificationSurface.viewport.width, height: notificationSurface.viewport.height, opacity: notificationSurface.viewport.presentationOpacity}) }
        function notifications(): void { if (notificationState.allEntries.length > 0) notificationCentre.visible = !notificationCentre.visible }
        function keyboard(): void { inputSourceSelector.openMenu(); }
        function keyboardNext(): void { inputSourceSelector.cycleSource(false); }
        function keyboardPrevious(): void { inputSourceSelector.cycleSource(true); }
        function controls(): void { controlCentre.visible = !controlCentre.visible }
        function capture(): void { captureTool.open() }
    }

    ControlCentre { id: controlCentre; widgetLayout: topBar; onWidgetEditRequested: (id, item) => root.openWidgetMenu(id, item, item.editWindow || item.barWindow || controlCentre); onCustomiseRequested: binguxSettings.openCustomise("", ""); anchorWindow: DesktopEditing.active ? topBar : controlCentre.movedAnchor?.barWindow || (topBar.zoneFor(systemPill) === "control-centre" ? topBar : topBar.windowFor(controlCentre.movedAnchor || systemPill)); anchorItem: DesktopEditing.active ? null : controlCentre.movedAnchor || (topBar.zoneFor(systemPill) === "control-centre" ? null : systemPill); dockSafeInset: Math.max(Theme.dockExclusiveHeight, dock.dockTopFromBottom); indicators: systemIndicators; screen: topBar.screen; onVisibleChanged: if (visible) root.closePanelsExcept(controlCentre) }

    SystemMetricsPopup {
        id: metricsPopup
        anchorWindow: topBar.windowFor(metricsPill)
        anchorItem: metricsPill
        anchorAlignment: Qt.AlignHCenter
        monitorWidget: metricsPill
        screen: topBar.screen
        onVisibleChanged: if (visible) root.closePanelsExcept(metricsPopup)
    }

    NotificationHistoryPopup {
        id: notificationCentre
        notificationSurface: notificationSurface
        screen: topBar.screen
        dockSafeInset: Math.max(Theme.dockExclusiveHeight, dock.dockTopFromBottom)
        onVisibleChanged: if (visible) root.closePanelsExcept(notificationCentre)
    }

    ShellPopup {
        id: barOverflow
        anchorWindow: topBar.windowFor(overflowButton)
        anchorItem: overflowButton
        screen: topBar.screen
        popupWidth: Math.max(200, ...topBar.overflowItems.map(item => item.implicitWidth + contentPadding * 2))
        popupHeight: overflowColumn.implicitHeight + contentPadding * 2
        contentPadding: Theme.gap
        preferredY: Theme.barHeight + Theme.gap
        onVisibleChanged: if (visible) {
            root.closePanelsExcept(barOverflow);
            Qt.callLater(() => overflowColumn.forceActiveFocus());
        }
        GridLayout {
            id: overflowColumn
            columns: 1
            width: parent.width
            rowSpacing: Theme.gap
            Keys.onEscapePressed: barOverflow.visible = false
        }
    }

    CalendarPopup {
        id: calendarPopup
        screen: topBar.screen
        anchorWindow: topBar.windowFor(clockPill)
        anchorItem: clockPill
        anchorAlignment: Qt.AlignHCenter
        onVisibleChanged: if (visible) root.closePanelsExcept(calendarPopup)
    }

    PanelWindow {
        id: topBar
        function snapshotLayout() {
            if (customLayout) return customLayout;
            const layout = DesktopLayout.defaults();
            layout["top-right"] = orderedControls.filter(item => chosen(item) && ![searchPill, clockPill].includes(item)).map(item => controlNames[defaultControls.indexOf(item)]);
            layout.sidebar = terminalSidebar.contentTypes.map(type => type.id);
            return layout;
        }
        readonly property var customLayout: DesktopEditing.desktop.layout
        readonly property string spacingKey: customLayout ? ["top-left", "top-center", "top-right"].reduce((items, zone) => items.concat(customLayout[zone]), []).filter(DesktopLayout.isSpacing).join(";") : ""
        readonly property var spacingIds: spacingKey ? spacingKey.split(";") : []
        property var spacingWidgets: []
        ListModel { id: spacingInstances }
        ListModel { id: decorationInstances }
        function reconcileInstances(model, ids) {
            for (let index = model.count - 1; index >= 0; index--)
                if (!ids.includes(model.get(index).instanceId)) model.remove(index);
            const existing = [];
            for (let index = 0; index < model.count; index++) existing.push(model.get(index).instanceId);
            for (const id of ids) if (!existing.includes(id)) model.append({instanceId: id});
        }
        function syncWidgetInstances() {
            reconcileInstances(spacingInstances, spacingIds);
            reconcileInstances(decorationInstances, decorationKey ? decorationKey.split(";") : []);
        }
        // A cross-container move updates two lists. Reconcile after both changes.
        onSpacingKeyChanged: Qt.callLater(syncWidgetInstances)
        onDecorationKeyChanged: Qt.callLater(syncWidgetInstances)
        Component.onCompleted: syncWidgetInstances()
        Instantiator {
            model: spacingInstances
            delegate: BarSpace {
                id: spacingWidget
                required property string instanceId
                readonly property string widgetId: instanceId
                visible: topBar.spacingWidgets.includes(spacingWidget) && topBar.chosen(spacingWidget)
                flexible: widgetId.startsWith("spring:")
                gapSize: DesktopEditing.desktop.widgetOptions?.[widgetId]?.width || 20
                parent: topBar.hostFor(spacingWidget)
                Layout.column: topBar.controlColumn(spacingWidget)
                Layout.row: 0
            }
            onObjectAdded: (index, object) => topBar.spacingWidgets = topBar.spacingWidgets.concat([object])
            onObjectRemoved: (index, object) => { object.parent = null; topBar.spacingWidgets = topBar.spacingWidgets.filter(item => item !== object); }
        }
        readonly property string decorationKey: customLayout ? ["top-left", "top-center", "top-right", "dock", "sidebar"].reduce((items, zone) => items.concat(customLayout[zone]), []).concat(DesktopEditing.desktop.controlLayout?.groups["control-centre"] || []).filter(DesktopLayout.isDecoration).join(";") : ""
        property var decorationWidgets: []
        Instantiator {
            model: decorationInstances
            delegate: DesktopDecoration {
                id: decorationWidget
                required property string instanceId
                widgetId: instanceId
                visible: topBar.decorationWidgets.includes(decorationWidget) && topBar.chosen(decorationWidget)
                parent: topBar.hostFor(decorationWidget)
                Layout.column: topBar.controlColumn(decorationWidget)
                Layout.row: topBar.controlRow(decorationWidget)
            }
            onObjectAdded: (index, object) => topBar.decorationWidgets = topBar.decorationWidgets.concat([object])
            onObjectRemoved: (index, object) => { object.parent = null; topBar.decorationWidgets = topBar.decorationWidgets.filter(item => item !== object); }
        }
        function hasSpring(zone) { return customLayout?.[zone]?.some(id => id.startsWith("spring:")) || false; }
        function zoneBudget(zone) {
            const centre = availableControls.filter(item => zoneFor(item) === "top-center");
            const centreWidth = hasSpring("top-center") ? width * 0.4 : Math.min(width * 0.4, centre.reduce((sum, item) => sum + item.implicitWidth + Theme.barControlGap, 0));
            return zone === "top-center" ? centreWidth : Math.max(0, (width - centreWidth) / 2 - Theme.gap);
        }
        function appearance(id, label, icon, nativeIcon, nativeText) {
            return DesktopLayout.presentation(DesktopEditing.desktop, id,
                customLayout ? DesktopLayout.placement(DesktopEditing.desktop, id) : "top-right", label, icon, nativeIcon, nativeText);
        }
        readonly property bool nativeTopBarLayout: !customLayout || (
            JSON.stringify(customLayout["top-left"]) === '["search"]' &&
            JSON.stringify(customLayout["top-center"]) === '["clock"]' &&
            customLayout.dock.length === 0 &&
            !["top-left", "top-center", "top-right"].some(zone => customLayout[zone].some(id => id.startsWith("control-") || id.startsWith("controls-") || DesktopLayout.isSpacing(id) || DesktopLayout.isDecoration(id))) &&
            ["top-left", "top-center", "top-right"].every(zone => !DesktopEditing.desktop.containers?.[zone]?.display || DesktopEditing.desktop.containers[zone].display === "native") &&
            Object.keys(DesktopEditing.desktop.widgetOptions || {}).every(id => !DesktopLayout.zone(customLayout, id).startsWith("top-") || !appearance(id, "", "", false, false).custom) &&
            ["capture", "tray", "privacy", "metrics", "keyboard", "controls", "notifications"].every(id => customLayout["top-right"].includes(id)))
        function nameFor(item) { return item.widgetId || controlNames[defaultControls.indexOf(item)]; }
        function chosen(item) {
            const name = item.widgetId || controlNames[defaultControls.indexOf(item)];
            return customLayout ? DesktopLayout.placement(DesktopEditing.desktop, name) !== "" : !DesktopLayout.widget(name)?.panel && !name.startsWith("control-") && !name.startsWith("controls-");
        }
        function zoneFor(item) {
            const name = item.widgetId || controlNames[defaultControls.indexOf(item)];
            if (name === "overflow" && (!customLayout || !DesktopLayout.placement(DesktopEditing.desktop, name))) return "top-right";
            if (!customLayout && DesktopLayout.widget(name)?.panel) return "sidebar";
            return customLayout ? DesktopLayout.placement(DesktopEditing.desktop, name) : name === "search" ? "top-left" : name === "clock" ? "top-center" : "top-right";
        }
        function windowFor(item) { return zoneFor(item) === "sidebar" ? terminalSidebar.widgetWindow : zoneFor(item) === "dock" ? dock : zoneFor(item) === "control-centre" ? (controlCentre.hostItem ? controlCentre.anchorWindow : controlCentre.nativeWindow) : topBar; }
        function hostFor(item) {
            if (overflows(item)) return overflowColumn;
            const zone = zoneFor(item);
            return zone === "sidebar" ? terminalSidebar.widgetHost : zone === "control-centre" ? controlCentre.widgetHost : zone === "dock" ? dock.widgetHost : zone === "top-left" ? leftControls : zone === "top-center" ? centerControls : rightControls;
        }
        readonly property real controlsBudget: Math.max(0, (width - clockPill.implicitWidth) / 2 - Theme.gap)
        // Display order is independent of overflow priority and reparenting order.
        readonly property var defaultControls: [captureStatus, trayContainer, privacyContainer,
            metricsPill, inputSourceSelector, overflowButton, systemPill, notificationButton, searchPill, clockPill].concat(controlCentre.movableWidgets, spacingWidgets, decorationWidgets, terminalSidebar.panelWidgets)
        readonly property var controlNames: ["capture", "tray", "privacy", "metrics", "keyboard", "overflow", "controls", "notifications", "search", "clock"].concat(controlCentre.movableWidgets.map(item => item.widgetId), spacingWidgets.map(item => item.widgetId), decorationWidgets.map(item => item.widgetId), terminalSidebar.panelWidgets.map(item => item.widgetId))
        Settings {
            id: barPreferences
            location: "file://" + (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/top-bar.ini"
            property string order: ""
        }
        readonly property var orderedControls: {
            const names = customLayout ? ["top-left", "top-center", "top-right", "dock"].reduce((names, zone) => names.concat(customLayout[zone]), []) : barPreferences.order.split(",");
            const ordered = [];
            for (const name of names) {
                const index = controlNames.indexOf(name);
                if (index >= 0 && !ordered.includes(defaultControls[index])) ordered.push(defaultControls[index]);
            }
            return ordered.concat(defaultControls.filter(item => !ordered.includes(item)));
        }
        function dropTarget(item, globalPoint) {
            const vertical = overflows(item);
            const siblings = (vertical ? overflowItems : barControls).filter(other => other !== item && other.parent === item.parent);
            const point = item.parent.mapFromGlobal(globalPoint.x, globalPoint.y);
            const coordinate = vertical ? point.y : point.x;
            const before = siblings.find(other => coordinate < (vertical ? other.y + other.height / 2 : other.x + other.width / 2));
            return { siblings: siblings, before: before, point: point };
        }
        property var draggedControl: null
        property var previewOrder: []
        property real dragOffset: 0
        property bool settlingReorder: false
        property bool committingReorder: false
        property bool cancelReorder: false
        function beginReorder(item) {
            if (draggedControl) return false;
            draggedControl = item;
            previewOrder = orderedControls.slice();
            dragOffset = 0;
            cancelReorder = false;
            return true;
        }
        function updateReorder(item, globalPoint, startPoint) {
            if (draggedControl !== item || settlingReorder) return;
            dragOffset = overflows(item) ? globalPoint.y - startPoint.y : globalPoint.x - startPoint.x;
            const target = dropTarget(item, globalPoint);
            if (!target.siblings.length) return;
            const order = orderedControls.filter(other => other !== item);
            const index = target.before ? order.indexOf(target.before)
                : order.indexOf(target.siblings[target.siblings.length - 1]) + 1;
            order.splice(index, 0, item);
            previewOrder = order;
        }
        function reorderShift(item) {
            if (!draggedControl || item.parent !== draggedControl.parent) return 0;
            const vertical = overflows(draggedControl);
            const siblings = (vertical ? overflowItems : barControls).filter(other => other.parent === item.parent);
            const preview = previewOrder.filter(other => siblings.includes(other));
            if (!preview.length || !preview.includes(item)) return 0;
            let position = vertical ? siblings[0].y : siblings[0].x;
            for (const other of preview) {
                if (other === item) return position - (vertical ? item.y : item.x);
                position += (vertical ? other.height : other.width) + (vertical ? Theme.gap : Theme.barControlGap);
            }
            return 0;
        }
        function finishReorder(item, globalPoint, cancelled) {
            if (draggedControl !== item || settlingReorder) return;
            const point = item.parent.mapFromGlobal(globalPoint.x, globalPoint.y);
            cancelReorder = cancelled || point.x < -Theme.gap || point.y < -Theme.gap
                || point.x > item.parent.width + Theme.gap || point.y > item.parent.height + Theme.gap;
            if (cancelReorder) previewOrder = orderedControls.slice();
            settlingReorder = true;
            reorderSettle.from = dragOffset;
            reorderSettle.to = reorderShift(item);
            reorderSettle.restart();
        }
        function completeReorder() {
            // Apply the model and remove preview transforms in the same frame,
            // exactly as the dock does after its settle animation.
            committingReorder = true;
            if (!cancelReorder) {
                if (customLayout) {
                    const layout = JSON.parse(JSON.stringify(customLayout));
                    for (const zone of ["top-left", "top-center", "top-right", "dock"])
                        layout[zone] = previewOrder.filter(other => zoneFor(other) === zone && other !== overflowButton).map(other => controlNames[defaultControls.indexOf(other)]);
                    BinguxPreferences.saveLayout(layout);
                } else {
                    barPreferences.order = previewOrder.map(other => controlNames[defaultControls.indexOf(other)]).join(",");
                    barPreferences.sync();
                }
            }
            draggedControl = null;
            previewOrder = [];
            dragOffset = 0;
            settlingReorder = false;
            Qt.callLater(() => { committingReorder = false; });
        }
        NumberAnimation {
            id: reorderSettle
            target: topBar
            property: "dragOffset"
            duration: Theme.reducedMotion ? 0 : 180
            easing.type: Easing.OutCubic
            onFinished: topBar.completeReorder()
        }
        readonly property var availableControls: [
            [captureStatus, captureStatus.visible], [trayContainer, tray.implicitWidth > 0],
            [privacyContainer, privacyContainer.active], [metricsPill, profileSettings.metricsEnabled],
            [inputSourceSelector, metrics.desktopStateAvailable], [systemPill, true],
            [notificationButton, notificationState.allEntries.length > 0], [searchPill, true], [clockPill, true]
        ].filter(entry => entry[1] && chosen(entry[0])).map(entry => entry[0]).concat(controlCentre.movableWidgets.filter(item => chosen(item)), spacingWidgets.filter(item => chosen(item)), decorationWidgets.filter(item => chosen(item)), terminalSidebar.panelWidgets.filter(item => item.placed))
        readonly property var overflowItems: {
            if (customLayout && !nativeTopBarLayout) {
                const hidden = [];
                const center = availableControls.filter(item => zoneFor(item) === "top-center");
                const left = availableControls.filter(item => zoneFor(item) === "top-left");
                const right = availableControls.filter(item => zoneFor(item) === "top-right");
                const demand = items => items.reduce((sum, item) => sum + (item.flexible ? 8 : item.implicitWidth), 0) + Math.max(0, items.length - 1) * Theme.barControlGap;
                const centerBudget = left.length || right.length ? width * 0.4 : width - 2 * Theme.gap;
                const sideBudget = center.length ? (width - (hasSpring("top-center") ? centerBudget : Math.min(centerBudget, demand(center)))) / 2 - Theme.gap : width / 2 - Theme.gap;
                for (const group of [{items: center, budget: centerBudget},
                    {items: left, budget: !center.length && !right.length ? width - Theme.barEdgeHitWidth - 2 * Theme.gap : sideBudget},
                    {items: right, budget: (!center.length && !left.length ? width - Theme.gap : sideBudget) - Theme.barEdgeHitWidth - Theme.barControlGap}]) {
                    const shown = group.items.slice();
                    for (const item of [trayContainer, metricsPill].concat(group.items.slice().reverse())) {
                        if (demand(shown) <= group.budget) break;
                        const index = shown.indexOf(item);
                        if (index < 0 || DesktopLayout.isSpacing(item.widgetId || "")) continue;
                        shown.splice(index, 1); hidden.push(item);
                    }
                }
                return orderedControls.filter(item => hidden.includes(item));
            }
            const items = availableControls.filter(item => zoneFor(item) === "top-right");
            let needed = items.reduce((sum, item) => sum + item.implicitWidth, 0)
                + Math.max(0, items.length - 1) * Theme.barControlGap;
            if (needed <= controlsBudget) return [];
            const hidden = [];
            // Keep primary actions accessible; secondary status moves into More.
            for (const item of [trayContainer, metricsPill]) {
                if (needed + Theme.barEdgeHitWidth + Theme.barControlGap <= controlsBudget) break;
                if (!items.includes(item)) continue;
                hidden.push(item);
                needed -= item.implicitWidth + Theme.barControlGap;
            }
            return orderedControls.filter(item => hidden.includes(item));
        }
        readonly property var barControls: orderedControls.filter(item => (item === overflowButton ? overflowItems.length > 0 : availableControls.includes(item)) && !overflows(item))
        function overflows(item) { return overflowItems.includes(item); }
        function controlColumn(item) { if (["sidebar", "control-centre"].includes(zoneFor(item))) return 0; return overflows(item) ? 0 : Math.max(0, barControls.filter(other => zoneFor(other) === zoneFor(item)).indexOf(item)); }
        function controlRow(item) { if (zoneFor(item) === "sidebar") return terminalSidebar.widgetRow(nameFor(item)); if (zoneFor(item) === "control-centre") return controlCentre.sectionRow(nameFor(item)); return overflows(item) ? overflowItems.indexOf(item) : 0; }
        onOverflowItemsChanged: if (overflowItems.length === 0) barOverflow.visible = false
        margins.left: terminalSidebar.leftInset
        margins.right: terminalSidebar.rightInset
        exclusiveZone: Theme.barHeight
        implicitHeight: Theme.barHeight
        color: "transparent"
        WlrLayershell.layer: DesktopEditing.active || !!popouts.states.search?.revealCompanions ? WlrLayer.Overlay : WlrLayer.Top
        WlrLayershell.namespace: "bingux-top-bar"
        WlrLayershell.keyboardFocus: DesktopEditing.active || !!popouts.states.search?.revealCompanions ? WlrKeyboardFocus.None : WlrKeyboardFocus.OnDemand
        anchors { top: true; left: true; right: true }

        Rectangle {
            anchors.fill: parent
            color: Theme.barBackground
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.leftMargin: terminalSidebar.leftInset > 0 ? terminalSidebar.desktopCornerSize : 0
                anchors.rightMargin: terminalSidebar.rightInset > 0 ? terminalSidebar.desktopCornerSize : 0
                visible: terminalSidebar.topInset === 0
                height: 1
                color: Theme.barDivider
            }
        }
        readonly property real editLeftBoundary: Math.max(leftControls.width, Math.min(centerControls.x, (leftControls.width + centerControls.x) / 2))
        readonly property real editRightBoundary: Math.min(width - rightControls.width, Math.max(centerControls.x + centerControls.width, (centerControls.x + centerControls.width + width - rightControls.width) / 2))
        NativeEditSurface {
            window: topBar; zoneName: "top-left"; x: 0; width: topBar.editLeftBoundary; height: topBar.height
            entries: topBar.defaultControls.filter(item => topBar.zoneFor(item) === zoneName).map(item => ({id: topBar.controlNames[topBar.defaultControls.indexOf(item)], item}))
        }
        NativeEditSurface {
            window: topBar; zoneName: "top-center"; x: topBar.editLeftBoundary; width: Math.max(0, topBar.editRightBoundary - x); height: topBar.height
            entries: topBar.defaultControls.filter(item => topBar.zoneFor(item) === zoneName).map(item => ({id: topBar.controlNames[topBar.defaultControls.indexOf(item)], item}))
        }
        NativeEditSurface {
            window: topBar; zoneName: "top-right"; x: topBar.editRightBoundary; width: topBar.width - x; height: topBar.height
            entries: topBar.defaultControls.filter(item => topBar.zoneFor(item) === zoneName).map(item => ({id: topBar.controlNames[topBar.defaultControls.indexOf(item)], item}))
        }
        Item {
            anchors.fill: parent
            GridLayout { id: leftControls; width: topBar.hasSpring("top-left") ? topBar.zoneBudget("top-left") : implicitWidth; anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; rows: 1; columnSpacing: Theme.barControlGap }
            GridLayout { id: centerControls; width: topBar.hasSpring("top-center") ? topBar.zoneBudget("top-center") : implicitWidth; anchors.horizontalCenter: parent.horizontalCenter; anchors.verticalCenter: parent.verticalCenter; rows: 1; columnSpacing: Theme.barControlGap }
            BarSearchButton {
                id: searchPill
                presentation: topBar.appearance("search", "Search", "system-search-symbolic", true, false)
                parent: topBar.hostFor(searchPill)
                Layout.column: topBar.controlColumn(searchPill); Layout.row: topBar.controlRow(searchPill)
                visible: topBar.chosen(searchPill)
                onClicked: root.openSearch()
            }
            Pill {
                id: clockPill
                presentation: topBar.appearance("clock", clockLabel.text + " " + timeLabel.text, "x-office-calendar-symbolic", false, true)
                parent: topBar.hostFor(clockPill)
                Layout.column: topBar.controlColumn(clockPill); Layout.row: topBar.controlRow(clockPill)
                visible: topBar.chosen(clockPill)
                z: 1
                horizontalPadding: Theme.barPrimaryPadding
                interactive: true
                hovered: clockMouse.containsMouse
                pressed: clockMouse.pressed
                selected: calendarPopup.visible
                activeFocusOnTab: true
                Accessible.onPressAction: calendarPopup.visible = !calendarPopup.visible
                Accessible.name: "Calendar, " + clockLabel.text + " " + timeLabel.text
                Accessible.role: Accessible.Button
                Keys.onReturnPressed: calendarPopup.visible = !calendarPopup.visible
                Keys.onSpacePressed: calendarPopup.visible = !calendarPopup.visible
                RollingNumber { id: clockLabel; text: root.currentTime.toLocaleDateString(Qt.locale(), "ddd d MMM"); value: root.currentTime.getTime(); pixelSize: Theme.fontSize; fontWeight: Font.DemiBold }
                RollingNumber { id: timeLabel; text: root.currentTime.toLocaleTimeString(Qt.locale(), "hh:mm:ss"); value: root.currentTime.getTime(); pixelSize: Theme.fontSize; fontWeight: Font.DemiBold }
                MouseArea { id: clockMouse; parent: clockPill; anchors.fill: parent; hoverEnabled: true; onClicked: calendarPopup.visible = !calendarPopup.visible }
            }
            Item {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: Math.max(0, (parent.width - clockPill.width) / 2 - Theme.gap)
                GridLayout {
                    id: rightControls
                    width: topBar.hasSpring("top-right") ? topBar.zoneBudget("top-right") : implicitWidth
                    Instantiator {
                        model: topBar.defaultControls.filter(item => !controlCentre.movableWidgets.includes(item) && !terminalSidebar.panelWidgets.includes(item))
                        delegate: WidgetEditHandle {
                            required property var modelData
                            control: modelData
                            widgetId: modelData.widgetId || topBar.controlNames[topBar.defaultControls.indexOf(modelData)] || ""
                            onRequested: (id, item) => root.openWidgetMenu(id, item, topBar.windowFor(item))
                        }
                    }
                    rows: 1
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    columnSpacing: Theme.barControlGap
                    RecordingIndicator {
                        id: captureStatus
                        presentation: topBar.appearance("capture", label || tooltip, "media-record-symbolic", false, true)
                        parent: topBar.hostFor(captureStatus)
                        Layout.column: topBar.controlColumn(captureStatus)
                        Layout.row: topBar.controlRow(captureStatus)
                        capture: captureTool
                        privacy: privacySession
                        visible: topBar.chosen(captureStatus) && (captureTool.busy || privacySession.recording)
                        barWindow: topBar.windowFor(captureStatus)
                        reorderable: true
                    }
                    Pill { id: trayContainer; panelLayout: ["sidebar", "control-centre"].includes(topBar.zoneFor(trayContainer)); parent: topBar.hostFor(trayContainer); Layout.column: topBar.controlColumn(trayContainer); Layout.row: topBar.controlRow(trayContainer); horizontalPadding: 0; visible: topBar.chosen(trayContainer) && tray.implicitWidth > 0; Tray { id: tray; panelLayout: trayContainer.panelLayout; presentation: topBar.appearance("tray", "System tray", "view-more-symbolic", true, false); parentWindow: topBar.windowFor(trayContainer) } }
                    PrivacyIndicators {
                        id: privacyContainer
                        parent: topBar.hostFor(privacyContainer)
                        Layout.column: topBar.controlColumn(privacyContainer)
                        Layout.row: topBar.controlRow(privacyContainer)
                        systemMetrics: metrics
                        privacyState: privacySession
                        visible: topBar.chosen(privacyContainer) && active
                        barWindow: topBar.windowFor(privacyContainer)
                    }
                    SystemMetrics {
                        id: metricsPill
                        panelLayout: ["sidebar", "control-centre"].includes(topBar.zoneFor(metricsPill))
                        presentation: topBar.appearance("metrics", "System monitors", "computer-symbolic", true, true)
                        parent: topBar.hostFor(metricsPill); Layout.column: topBar.controlColumn(metricsPill); Layout.row: topBar.controlRow(metricsPill)
                        systemMetrics: metrics
                        visible: topBar.chosen(metricsPill) && profileSettings.metricsEnabled
                        selected: metricsPopup.visible
                        onConfigureRequested: {
                            metricsPopup.showPage(true);
                        }
                        onPerformanceRequested: {
                            metricsPopup.showPage(false);
                        }
                        BarTooltip { reorderable: true; anchorItem: metricsPill; barWindow: topBar.windowFor(metricsPill); requested: metricsPill.pointerHovered; text: metricsPill.description }
                    }
                    InputSourceSelector { id: inputSourceSelector; presentation: topBar.appearance("keyboard", displayLabel, "input-keyboard-symbolic", false, true); parent: topBar.hostFor(inputSourceSelector); Layout.column: topBar.controlColumn(inputSourceSelector); Layout.row: topBar.controlRow(inputSourceSelector); visible: topBar.chosen(inputSourceSelector) && metrics.desktopStateAvailable; parentWindow: topBar.windowFor(inputSourceSelector); metrics: metrics; gnoblinCtlPath: profileSettings.gnoblinCtlPath; onOpening: root.closePanelsExcept(inputSourceSelector) }
                    Pill {
                        id: systemPill
                        presentation: topBar.appearance("controls", "Control centre", "preferences-system-symbolic", true, false)
                        parent: topBar.hostFor(systemPill); Layout.column: topBar.controlColumn(systemPill); Layout.row: topBar.controlRow(systemPill)
                        visible: topBar.chosen(systemPill)
                        horizontalPadding: Theme.barPrimaryPadding
                        interactive: true
                        hovered: systemMouse.containsMouse
                        pressed: systemMouse.pressed
                        selected: controlCentre.visible
                        activeFocusOnTab: true
                        Accessible.onPressAction: controlCentre.visible = !controlCentre.visible
                        Accessible.name: "Control Centre"
                        Accessible.description: systemIndicators.extraStatusDescription
                        Accessible.role: Accessible.Button
                        Keys.onReturnPressed: controlCentre.visible = !controlCentre.visible
                        Keys.onSpacePressed: controlCentre.visible = !controlCentre.visible
                        SystemIndicators { id: systemIndicators; timeoutPath: profileSettings.timeoutPath }
                        BarTooltip { reorderable: true; anchorItem: systemPill; barWindow: topBar.windowFor(systemPill); requested: systemMouse.containsMouse; text: "Control Centre" + (systemIndicators.extraStatusDescription ? "\n" + systemIndicators.extraStatusDescription : "") }
                        MouseArea { id: systemMouse; parent: systemPill; anchors.fill: parent; hoverEnabled: true; onClicked: controlCentre.visible = !controlCentre.visible }
                    }
                    BarOverflowButton {
                        id: overflowButton
                        parent: topBar.hostFor(overflowButton)
                        presentation: topBar.appearance("overflow", "More", "view-more-symbolic", true, false)
                        Layout.column: topBar.controlColumn(overflowButton)
                        Layout.row: topBar.controlRow(overflowButton)
                        visible: topBar.overflowItems.length > 0
                        selected: barOverflow.visible
                        barWindow: topBar.windowFor(overflowButton)
                        onClicked: barOverflow.visible = !barOverflow.visible
                    }
                    BarNotificationButton {
                        id: notificationButton
                        presentation: topBar.appearance("notifications", "Notifications", "preferences-system-notifications-symbolic", true, false)
                        parent: topBar.hostFor(notificationButton); Layout.column: topBar.controlColumn(notificationButton); Layout.row: topBar.controlRow(notificationButton)
                        visible: topBar.chosen(notificationButton) && count > 0
                        count: notificationState.allEntries.length
                        selected: notificationCentre.visible
                        barWindow: topBar.windowFor(notificationButton)
                        onClicked: if (count > 0) notificationCentre.visible = !notificationCentre.visible
                        Connections {
                            target: notificationSurface.viewport
                            function onToastArchived() { notificationButton.playArchive(); }
                        }
                    }
                }
            }
        }
    }

    Dock {
        id: dock
        WlrLayershell.layer: DesktopEditing.active || !!popouts.states.search?.revealCompanions ? WlrLayer.Overlay : WlrLayer.Top
        onApplicationInteracted: popouts.command("search", {action: "hide"})
        onWidgetEditRequested: (id, item) => root.openWidgetMenu(id, item, dock)
        notifications: notificationState.allEntries
        notificationStore: notificationState
        margins.left: terminalSidebar.leftInset
        margins.right: terminalSidebar.rightInset
        settings: profileSettings
        visible: DesktopEditing.active || profileSettings.dockEnabled
    }

}
