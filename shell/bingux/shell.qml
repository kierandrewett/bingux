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

ShellRoot {
    id: root

    readonly property var commandNotifications: notificationState
    property var currentTime: new Date()

    function closePanelsExcept(panel) {
        if (panel !== windowSwitcher && windowSwitcher.active) windowSwitcher.close();
        if (panel !== searchOverlay && searchOverlay.visible) searchOverlay.closeSearch();
        if (panel !== emojiPicker) emojiPicker.visible = false;
        if (panel !== calendarPopup) calendarPopup.visible = false;
        if (panel !== controlCentre) controlCentre.visible = false;
        if (panel !== metricsPopup) metricsPopup.visible = false;
        if (panel !== barOverflow) barOverflow.visible = false;
        if (panel !== notificationCentre) notificationCentre.visible = false;
        if (panel !== inputSourceSelector) inputSourceSelector.menuOpen = false;
        if (panel !== captureTool && captureTool.opened) captureTool.close();
    }

    function openSearch() {
        searchOverlay.showSearch();
    }

    ProfileSettings {
        id: profileSettings
    }

    TerminalSidebar {
        id: terminalSidebar
        systemMetrics: metrics
        settings: profileSettings
        screen: topBar.screen
        inputSuspended: captureTool.opened
    }

    Metrics {
        id: metrics
    }

    SearchOverlay {
        onSettingsRequested: binguxSettings.visible = true
        id: searchOverlay
        onVisibleChanged: if (visible) root.closePanelsExcept(searchOverlay)
    }

    PrivacyState { id: privacySession }

    WindowSwitcher {
        id: windowSwitcher
        activeStreams: dock.activity.activeStreams
        notifications: notificationState.allEntries
        onOpening: { root.closePanelsExcept(windowSwitcher); dock.closeMenus?.(); }
    }

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
        inputSuspended: captureTool.opened
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

    BinguxSettings { id: binguxSettings }

    IpcHandler {
        target: "shell"
        function status(): string {
            return JSON.stringify({search: searchOverlay.visible, calendar: calendarPopup.visible,
                controls: controlCentre.visible, notifications: notificationCentre.visible,
                metrics: metricsPopup.visible, keyboard: inputSourceSelector.menuOpen,
                capture: captureTool.state, sidebar: terminalSidebar.opened,
                doNotDisturb: ControlCentreServices.doNotDisturb});
        }
        function reload(): void { Qt.callLater(() => Quickshell.reload(false)); }
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

    ControlCentre { id: controlCentre; anchorWindow: topBar; anchorItem: systemPill; dockSafeInset: Math.max(Theme.dockExclusiveHeight, dock.dockTopFromBottom); indicators: systemIndicators; screen: topBar.screen; onVisibleChanged: if (visible) root.closePanelsExcept(controlCentre) }

    SystemMetricsPopup {
        id: metricsPopup
        anchorWindow: topBar
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
        anchorWindow: topBar
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
        anchorWindow: topBar
        anchorItem: clockPill
        anchorAlignment: Qt.AlignHCenter
        onVisibleChanged: if (visible) root.closePanelsExcept(calendarPopup)
    }

    PanelWindow {
        id: topBar
        readonly property real controlsBudget: Math.max(0, (width - clockPill.implicitWidth) / 2 - Theme.gap)
        // Display order is independent of overflow priority and reparenting order.
        readonly property var defaultControls: [captureStatus, trayContainer, privacyContainer,
            metricsPill, inputSourceSelector, overflowButton, systemPill, notificationButton]
        readonly property var controlNames: ["capture", "tray", "privacy", "metrics", "keyboard", "overflow", "controls", "notifications"]
        Settings {
            id: barPreferences
            location: "file://" + (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/top-bar.ini"
            property string order: ""
        }
        readonly property var orderedControls: {
            const names = barPreferences.order.split(",");
            const ordered = [];
            for (const name of names) {
                const index = controlNames.indexOf(name);
                if (index >= 0 && !ordered.includes(defaultControls[index])) ordered.push(defaultControls[index]);
            }
            return ordered.concat(defaultControls.filter(item => !ordered.includes(item)));
        }
        function dropTarget(item, globalPoint) {
            const vertical = overflows(item);
            const siblings = (vertical ? overflowItems : barControls).filter(other => other !== item);
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
            const siblings = vertical ? overflowItems : barControls;
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
                barPreferences.order = previewOrder.map(other => controlNames[defaultControls.indexOf(other)]).join(",");
                barPreferences.sync();
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
            [notificationButton, notificationState.allEntries.length > 0]
        ].filter(entry => entry[1]).map(entry => entry[0])
        readonly property var overflowItems: {
            const items = availableControls;
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
        function controlColumn(item) { return overflows(item) ? 0 : Math.max(0, barControls.indexOf(item)); }
        function controlRow(item) { return overflows(item) ? overflowItems.indexOf(item) : 0; }
        onOverflowItemsChanged: if (overflowItems.length === 0) barOverflow.visible = false
        margins.left: terminalSidebar.leftInset
        margins.right: terminalSidebar.rightInset
        exclusiveZone: Theme.barHeight
        implicitHeight: Theme.barHeight
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "bingux-top-bar"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
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
        Item {
            anchors.fill: parent
            Item {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: searchPill.implicitWidth
                BarSearchButton {
                    id: searchPill
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: implicitWidth
                    onClicked: root.openSearch()
                }
            }
            Pill {
                id: clockPill
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.verticalCenter: parent.verticalCenter
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
                    Instantiator {
                        model: topBar.defaultControls
                        delegate: BarReorderHandle {
                            required property var modelData
                            control: modelData
                            controller: topBar
                        }
                    }
                    rows: 1
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    columnSpacing: Theme.barControlGap
                    RecordingIndicator {
                        id: captureStatus
                        parent: rightControls
                        Layout.column: topBar.controlColumn(captureStatus)
                        Layout.row: topBar.controlRow(captureStatus)
                        capture: captureTool
                        privacy: privacySession
                        barWindow: topBar
                        reorderable: true
                    }
                    Pill { id: trayContainer; parent: topBar.overflows(trayContainer) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(trayContainer); Layout.row: topBar.controlRow(trayContainer); horizontalPadding: 0; visible: tray.implicitWidth > 0; Tray { id: tray; parentWindow: topBar } }
                    PrivacyIndicators {
                        id: privacyContainer
                        parent: rightControls
                        Layout.column: topBar.controlColumn(privacyContainer)
                        Layout.row: topBar.controlRow(privacyContainer)
                        systemMetrics: metrics
                        privacyState: privacySession
                        barWindow: topBar
                    }
                    SystemMetrics {
                        id: metricsPill
                        parent: topBar.overflows(metricsPill) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(metricsPill); Layout.row: topBar.controlRow(metricsPill)
                        systemMetrics: metrics
                        visible: profileSettings.metricsEnabled
                        selected: metricsPopup.visible
                        onConfigureRequested: {
                            metricsPopup.showPage(true);
                        }
                        onPerformanceRequested: {
                            metricsPopup.showPage(false);
                        }
                        BarTooltip { reorderable: true; anchorItem: metricsPill; barWindow: topBar; requested: metricsPill.pointerHovered; text: metricsPill.description }
                    }
                    InputSourceSelector { id: inputSourceSelector; parent: topBar.overflows(inputSourceSelector) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(inputSourceSelector); Layout.row: topBar.controlRow(inputSourceSelector); visible: metrics.desktopStateAvailable; parentWindow: topBar; metrics: metrics; gnoblinCtlPath: profileSettings.gnoblinCtlPath; onOpening: root.closePanelsExcept(inputSourceSelector) }
                    Pill {
                        id: systemPill
                        parent: topBar.overflows(systemPill) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(systemPill); Layout.row: topBar.controlRow(systemPill)
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
                        BarTooltip { reorderable: true; anchorItem: systemPill; barWindow: topBar; requested: systemMouse.containsMouse; text: "Control Centre" + (systemIndicators.extraStatusDescription ? "\n" + systemIndicators.extraStatusDescription : "") }
                        MouseArea { id: systemMouse; parent: systemPill; anchors.fill: parent; hoverEnabled: true; onClicked: controlCentre.visible = !controlCentre.visible }
                    }
                    AbstractButton {
                        id: overflowButton
                        Layout.column: topBar.controlColumn(overflowButton)
                        Layout.row: 0
                        visible: topBar.overflowItems.length > 0
                        implicitWidth: Theme.barEdgeHitWidth
                        implicitHeight: Theme.barHeight
                        hoverEnabled: true
                        activeFocusOnTab: true
                        Accessible.name: "More status controls"
                        onClicked: barOverflow.visible = !barOverflow.visible
                        Keys.onReturnPressed: clicked()
                        background: BarControlSurface { hovered: overflowButton.hovered; pressed: overflowButton.down; selected: barOverflow.visible; focused: overflowButton.visualFocus }
                        contentItem: Item {
                            SymbolicIcon { anchors.centerIn: parent; implicitSize: Theme.iconSize; source: Quickshell.iconPath("view-more-symbolic") }
                        }
                        BarTooltip { reorderable: true; anchorItem: overflowButton; barWindow: topBar; requested: overflowButton.hovered; text: "More status controls" }
                    }
                    AbstractButton {
                        id: notificationButton
                        parent: topBar.overflows(notificationButton) ? overflowColumn : rightControls; Layout.column: topBar.controlColumn(notificationButton); Layout.row: topBar.controlRow(notificationButton)
                        visible: notificationState.allEntries.length > 0
                        implicitWidth: notificationCount.implicitWidth + Theme.barEdgeHitWidth - Theme.iconSize
                        hoverEnabled: true
                        implicitHeight: Theme.barHeight
                        Accessible.name: "Notifications"
                        Accessible.description: notificationState.allEntries.length + " notifications"
                        onClicked: if (notificationState.allEntries.length > 0) notificationCentre.visible = !notificationCentre.visible
                        contentItem: Item {
                            NotificationIndicator {
                                id: notificationCount
                                Connections {
                                    target: notificationSurface.viewport
                                    function onToastArchived() { notificationCount.playArchive(); }
                                }
                                anchors.centerIn: parent
                                count: notificationState.allEntries.length
                            }
                        }
                        BarTooltip { reorderable: true; anchorItem: notificationButton; barWindow: topBar; requested: notificationButton.hovered; text: notificationState.allEntries.length > 0 ? "Notifications · " + notificationState.allEntries.length : "No notifications" }
                        background: BarControlSurface {
                            hovered: notificationButton.hovered
                            pressed: notificationButton.down
                            focused: notificationButton.activeFocus
                            selected: notificationCentre.visible
                        }
                    }
                }
            }
        }
    }

    Dock {
        id: dock
        notifications: notificationState.allEntries
        notificationStore: notificationState
        margins.left: terminalSidebar.leftInset
        margins.right: terminalSidebar.rightInset
        settings: profileSettings
        visible: profileSettings.dockEnabled
    }

}
