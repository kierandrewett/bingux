import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "DesktopLayout.js" as DesktopLayout

Scope {
    id: root
    signal widgetEditRequested(string widgetId, var control, var window)
    required property var settings
    property var systemMetrics: null
    property var widgetLayout: null
    readonly property alias widgetHost: widgetGrid
    readonly property var widgetWindow: floating ? detachedWindow : panel
    readonly property var externalEntries: widgetLayout ? widgetLayout.defaultControls
        .filter(item => widgetLayout.zoneFor(item) === "sidebar")
        .map(item => ({id: widgetLayout.nameFor(item), item})) : []
    function widgetRow(id) { return (DesktopEditing.desktop.layout?.sidebar || allContentTypes.map(type => type.id)).indexOf(id); }
    readonly property alias editWindow: panel
    readonly property alias editSurface: panelSurface
    readonly property alias contentItem: sidebarContents
    readonly property alias contentSelector: contentMenu
    readonly property alias detachedSurface: detachedWindow
    readonly property alias edgeSurface: sensor
    readonly property var allContentTypes: [
        {id: "terminal", label: "Terminal", icon: "utilities-terminal-symbolic"},
        {id: "notes", label: "Notes", icon: "accessories-text-editor-symbolic"},
        {id: "monitor", label: "System", icon: "computer-symbolic"},
        {id: "calendar", label: "Calendar", icon: "x-office-calendar-symbolic"},
        {id: "media", label: "Media", icon: "applications-multimedia-symbolic"},
        {id: "tasks", label: "Tasks", icon: "view-list-symbolic"}
    ]
    readonly property var contentTypes: DesktopEditing.desktop.layout ? DesktopEditing.desktop.layout.sidebar.map(id => allContentTypes.find(type => type.id === id)).filter(type => type) : allContentTypes
    Connections {
        target: BinguxPreferences
        function onDataChanged() {
            const edge = BinguxPreferences.data.desktop.sidebarEdge;
            if (edge && edge !== saved.edge) root.syncEdge(edge);
        }
    }
    property string previewContentType: ""
    readonly property string contentType: {
        const selected = DesktopEditing.active && previewContentType ? previewContentType : saved.contentType;
        return contentTypes.some(type => type.id === selected) ? selected : (contentTypes[0]?.id || "terminal");
    }
    readonly property var currentContent: contentTypes.find(type => type.id === contentType) || contentTypes[0] || allContentTypes[0]
    readonly property Item activePanel: contentType === "terminal" ? terminalLoader.item : contentType === "notes" ? notes : contentType === "monitor" ? monitor : extraPanel.item
    function focusContent() {
        if (contentType === "terminal" && terminalReady)
            terminalLoader.item.focusTerminal();
        else if (contentType === "notes")
            notes.focusContent();
        else if (contentType === "monitor")
            monitor.focusContent();
        else if (extraPanel.item)
            extraPanel.item.focusContent();
    }
    function selectContent(value) {
        if (!contentTypes.some(type => type.id === value))
            return;
        if (DesktopEditing.active) {
            previewContentType = value;
            if (value === "terminal") terminalCreated = true;
            return;
        }
        saved.contentType = value;
        saved.setValue("contentType", value);
        saved.sync();
        if (opened && value === "terminal")
            terminalCreated = true;
        if (opened)
            Qt.callLater(root.focusContent);
    }
    property var screen
    // Full-screen selectors own edge input without closing the terminal session.
    property bool inputSuspended: false
    // Layer surfaces can clear activeToplevel while taking keyboard focus.
    property var lastActiveWindow: ToplevelManager.activeToplevel
    Connections {
        target: ToplevelManager
        function onActiveToplevelChanged() {
            if (ToplevelManager.activeToplevel)
                root.lastActiveWindow = ToplevelManager.activeToplevel;
        }
    }
    readonly property var fullscreenWindow: lastActiveWindow && lastActiveWindow.fullscreen
        && !lastActiveWindow.minimized
        // Some compositors publish state without output enter/leave events.
        && (lastActiveWindow.screens.length === 0 || lastActiveWindow.screens.includes(screen)) ? lastActiveWindow : null
    readonly property bool fullscreenApp: fullscreenWindow !== null
    onFullscreenWindowChanged: {
        if (!fullscreenWindow || detached)
            return;
        gestureActive = false;
        dragging = false;
        useDragSize = false;
        handleVisible = false;
        hide();
    }
    property bool opened: false
    property alias detached: saved.detached
    // The editor uses the real sidebar at its edge without changing saved state.
    readonly property bool floating: detached && !DesktopEditing.active
    property bool focusRequested: false
    property bool handleVisible: false
    readonly property bool handleHovered: pillAllowed && (handleButton.hovered || (sensor.visible && sensorGesture.hovered) || gestureActive)
    property real pointerPosition: 200
    readonly property int closeThreshold: 150
    readonly property real followThreshold: 96
    property bool followingPointer: false
    property real followPosition: 200
    readonly property bool pillPainted: handleWindow.reveal > 0 && boundaryOpacity > 0
    property real animatedPosition: followPosition
    Behavior on animatedPosition {
        enabled: root.pillPainted
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 80
            easing.type: Easing.OutCubic
        }
    }
    function trackPointer(position) {
        const wasPainted = pillPainted;
        pointerPosition = position;
        if (!wasPainted) {
            followPosition = position;
            followingPointer = false;
            return;
        }
        if (gestureActive || Math.abs(position - followPosition) >= followThreshold)
            followingPointer = true;
        if (followingPointer)
            followPosition = position;
    }
    readonly property real usableTop: Theme.barHeight
    readonly property real usableBottom: (screen ? screen.height : 800) - (settings.dockEnabled ? Theme.dockExclusiveHeight : 0)
    // Limit both the edge sensor and the pill to the middle 40% of the screen.
    readonly property real handleAxisLength: edge === "top" ? (screen ? screen.width : 1280) : (screen ? screen.height : 800)
    readonly property real handleStart: Math.ceil(handleAxisLength * 0.3)
    readonly property real handleEnd: Math.floor(handleAxisLength * 0.7)
    readonly property bool pillAllowed: pointerPosition >= handleStart && pointerPosition < handleEnd && (edge !== "top" || handleOffset + 24 <= usableBottom)
    property real boundaryOpacity: pillAllowed ? 1 : 0
    Behavior on boundaryOpacity {
        NumberAnimation {
            duration: Theme.reducedMotion ? 0 : 160
            easing.type: Easing.OutCubic
        }
    }
    property bool gestureActive: false
    property bool dragging: false
    property bool useDragSize: false
    property real dragExtent: 0
    readonly property int maxSideWidth: Math.floor((screen ? screen.width : 1280) * 0.3)
    readonly property int minimumSideWidth: Math.min(250, maxSideWidth)
    readonly property real maximumExtent: edge === "top" ? (screen ? screen.height : 800) - Theme.barHeight : maxSideWidth
    readonly property int desktopCornerSize: fullscreenApp ? 0 : Math.round(Theme.shellRadius * 1.5 * Math.min(1, Math.max(0, panel.extent * panel.reveal) / Math.max(1, maximumExtent)))
    property real hintStrength: 0
    function hintDrag() {
        handleVisible = true;
        dismiss.stop();
        dragHint.restart();
    }
    SequentialAnimation {
        id: dragHint
        loops: 2
        NumberAnimation {
            target: root
            property: "hintStrength"
            to: 1
            duration: 120
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: root
            property: "hintStrength"
            to: 0
            duration: 260
            easing.type: Easing.InOutCubic
        }
        onStopped: dismiss.restart()
    }
    property real initialExtent: 0
    property real gestureOffset: 0
    property real gesturePosition: 0
    property real handleGrabPosition: 0
    property real sensorGrabOffset: 0
    property bool startedOpen: false
    readonly property real handleOffset: (edge === "top" ? Theme.barHeight : 0) + (panel.visible ? panel.extent * panel.reveal : 0)
    readonly property int topInset: edge === "top" && panel.visible ? Math.round(panel.extent * panel.reveal) : 0
    readonly property int leftInset: edge === "left" && panel.visible ? Math.round(panel.extent * panel.reveal) : 0
    readonly property int rightInset: edge === "right" && panel.visible ? Math.round(panel.extent * panel.reveal) : 0
    property real pressX: 0
    property real pressY: 0

    function beginGesture(x, y, crossPosition) {
        dismiss.stop();
        slide.stop();
        startedOpen = opened;
        initialExtent = opened ? panel.extent * panel.reveal : 0;
        gestureOffset = handleOffset;
        sensorGrabOffset = sensor.inputOffset;
        handleGrabPosition = animatedPosition;
        gesturePosition = crossPosition;
        dragExtent = initialExtent;
        pressX = x;
        pressY = y;
        gestureActive = true;
        trackPointer(crossPosition);
        handleVisible = true;
    }

    function updateGesture(x, y) {
        if (!gestureActive)
            return;
        trackPointer(gesturePosition + (edge === "top" ? x - pressX : y - pressY));
        const dx = x - pressX;
        const dy = y - pressY;
        if (!dragging && Math.hypot(dx, dy) < 6)
            return;
        const distance = edge === "top" ? dy : edge === "left" ? dx : -dx;
        const maximum = maximumExtent;
        dragExtent = Math.max(0, Math.min(maximum, initialExtent + distance));
        useDragSize = true;
        dragging = true;
        panel.reveal = dragExtent / panel.extent;
    }

    function finishGesture(cancelled) {
        if (!gestureActive)
            return;
        gestureActive = false;
        if (!dragging) {
            if (!cancelled)
                hintDrag();
            return;
        }
        if (cancelled && startedOpen) {
            useDragSize = false;
            open();
        } else if (!cancelled && Math.round(dragExtent) >= closeThreshold) {
            if (dragging) {
                if (edge === "top")
                    saved.heightFraction = dragExtent / (screen ? screen.height : 800);
                else
                    saved.widthFraction = Math.max(minimumSideWidth, dragExtent) / (screen ? screen.width : 1280);
            }
            useDragSize = false;
            open();
        } else {
            handleVisible = false;
            animateTo(0);
            opened = false;
            rememberOpen(false);
            focusRequested = false;
        }
        dragging = false;
    }

    // The input surface stays stationary throughout the pointer grab. Only the
    // separate, input-transparent pill moves, avoiding geometry feedback.
    component EdgeGesture: MouseArea {
        property bool tracksEdge: false
        readonly property bool hovered: containsMouse
        readonly property bool down: pressed
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        preventStealing: true
        onEntered: {
            dismiss.stop();
            if (tracksEdge)
                root.trackPointer(root.edge === "top" ? mouseX : mouseY);
            root.handleVisible = true;
        }
        onExited: if (!root.gestureActive)
            dismiss.restart()
        onPressed: mouse => {
            if (!root.pillAllowed) {
                mouse.accepted = false;
                return;
            }
            const point = mapToGlobal(mouse.x, mouse.y);
            const crossPosition = root.edge === "top" ? mouse.x + (tracksEdge ? 0 : handleWindow.baseLeft) : mouse.y + (tracksEdge ? 0 : handleWindow.baseTop);
            root.beginGesture(point.x, point.y, crossPosition);
        }
        onPositionChanged: mouse => {
            if (pressed) {
                const point = mapToGlobal(mouse.x, mouse.y);
                root.updateGesture(point.x, point.y);
            } else if (tracksEdge && !root.gestureActive)
                root.trackPointer(root.edge === "top" ? mouse.x : mouse.y);
        }
        onReleased: mouse => {
            const point = mapToGlobal(mouse.x, mouse.y);
            root.updateGesture(point.x, point.y);
            root.finishGesture(false);
        }
        onCanceled: root.finishGesture(true)
    }
    readonly property string edge: DesktopEditing.desktop.sidebarEdge ? DesktopEditing.desktop.sidebarEdge : ["left", "top", "right"].includes(saved.edge) ? saved.edge : "right"

    Settings {
        id: saved
        location: "file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar.ini"
        property bool wasOpen: false
        property bool detached: false
        property string edge: "right"
        property string contentType: "terminal"
        property real widthFraction: 0.3
        property real heightFraction: 0.4
    }

    Component.onCompleted: Qt.callLater(function () {
        if (saved.wasOpen && root.settings.sidebarEnabled && (!root.fullscreenApp || root.detached)) {
            root.terminalCreated = root.contentType === "terminal";
            root.opened = true;
            panel.reveal = 1;
        }
    })
    function rememberOpen(value) {
        saved.wasOpen = value;
        saved.setValue("wasOpen", value);
        saved.setValue("widthFraction", saved.widthFraction);
        saved.setValue("heightFraction", saved.heightFraction);
        saved.sync();
    }

    property bool terminalCreated: false
    readonly property bool terminalReady: terminalLoader.status === Loader.Ready && terminalLoader.item !== null

    Connections {
        target: root.settings
        ignoreUnknownSignals: true
        function onSidebarEnabledChanged() { if (!root.settings.sidebarEnabled) root.hide(); }
    }

    Connections {
        target: DesktopEditing
        function onActiveChanged() {
            root.previewContentType = "";
            contentMenu.visible = false;
            if (DesktopEditing.active && root.contentType === "terminal") root.terminalCreated = true;
            if (!root.floating) root.animateTo(DesktopEditing.active || root.opened ? 1 : 0);
        }
    }
    function open() {
        if (!settings.sidebarEnabled)
            return;
        handleVisible = false;
        if (contentType === "terminal")
            terminalCreated = true;
        focusRequested = true;
        opened = true;
        rememberOpen(true);
        animateTo(1);
        focusRelease.restart();
        Qt.callLater(root.focusContent);
    }

    function popOut() {
        contentMenu.visible = false;
        notes.closeContextMenu();
        detachedWindow.implicitWidth = Math.max(320, Math.min(panelSurface.width, 640));
        detachedWindow.implicitHeight = Math.max(360, Math.min(panelSurface.height, 720));
        saved.detached = true;
        saved.setValue("detached", true);
        saved.sync();
        if (!opened) open();
        slide.stop();
        panel.reveal = 1;
        focusRequested = false;
        Qt.callLater(root.focusContent);
    }
    function dockBack() {
        contentMenu.visible = false;
        notes.closeContextMenu();
        saved.detached = false;
        saved.setValue("detached", false);
        saved.sync();
        open();
    }
    function animateTo(value) {
        slide.stop();
        slide.from = panel.reveal;
        slide.to = value;
        slide.start();
    }

    function hide() {
        if (!opened)
            return;
        contentMenu.visible = false;
        focusRequested = false;
        animateTo(0);
        opened = false;
        rememberOpen(false);
    }

    function setEdge(value) {
        if (!["left", "top", "right"].includes(value)) return;
        if (DesktopEditing.active) DesktopEditing.editor.change("sidebarEdge", value);
        else if (BinguxPreferences.data.desktop.layoutVersion === 1) BinguxPreferences.saveDesktop({sidebarEdge: value});
        else syncEdge(value);
    }
    function syncEdge(value) {
        if (!["left", "top", "right"].includes(value))
            return;
        contentMenu.visible = false;
        if (root.detached && DesktopEditing.active) {
            saved.edge = value;
            saved.setValue("edge", value);
            saved.sync();
            return;
        }
        const wasOpen = opened;
        slide.stop();
        panel.reveal = 0;
        opened = false;
        if (root.detached) { saved.detached = false; saved.setValue("detached", false); }
        saved.edge = value;
        saved.setValue("edge", value);
        saved.sync();
        Qt.callLater(function () {
            if (wasOpen)
                root.open();
        });
    }

    Timer {
        id: focusRelease
        interval: 150
        onTriggered: root.focusRequested = false
    }
    NumberAnimation {
        id: slide
        target: panel
        property: "reveal"
        duration: Theme.reducedMotion ? 0 : 180
        easing.type: Easing.OutCubic
    }

    function restartTerminal() {
        terminalCreated = false;
        Qt.callLater(function () {
            root.terminalCreated = true;
        });
    }

    IpcHandler {
        target: "sidebar"
        function select(value: string): void { root.selectContent(value); }
        function popout(): void { root.popOut(); }
        function dock(): void { root.dockBack(); }
        function open(): void {
            root.open();
        }
        function hide(): void {
            root.hide();
        }
        function toggle(): void {
            if (root.opened)
                root.hide();
            else
                root.open();
        }
        function edge(value: string): void {
            root.setEdge(value);
        }
        function status(): string {
            return JSON.stringify({
                open: root.opened,
                detached: root.detached,
                sensorVisible: sensor.visible,
                sensorHovered: sensorGesture.hovered,
                sensorWidth: sensor.width,
                sensorHeight: sensor.height,
                pointerPosition: root.pointerPosition,
                handleHovered: handleButton.hovered,
                suspended: root.inputSuspended,
                fullscreen: root.fullscreenApp,
                reveal: panel.reveal,
                dragging: root.dragging,
                hinting: dragHint.running,
                pillVisible: handleWindow.visible && root.pillAllowed,
                pillHovered: root.handleHovered,
                pillOpacity: root.boundaryOpacity,
                surfaceWidth: panel.width,
                surfaceHeight: panel.height,
                contentX: root.edge === "right" ? panel.width - panel.extent : 0,
                contentY: root.edge === "top" ? Theme.barHeight : 0,
                contentHeight: panelSurface.height,
                edge: root.edge,
                contentType: root.contentType,
                menuOpen: contentMenu.visible,
                headerHeight: sidebarHeader.height,
                contentPadding: Theme.gap,
                cornerSize: root.desktopCornerSize,
                terminalTransparent: root.terminalReady && ("backgroundOpacity" in terminalLoader.item) && terminalLoader.item["backgroundOpacity"] === 0,
                ready: root.terminalReady,
                pid: root.terminalReady ? (terminalLoader.item?.shellPid ?? 0) : 0,
                running: root.terminalReady && terminalLoader.item?.shellRunning
            });
        }
    }

    PanelWindow {
        id: panel
        property real reveal: 0
        screen: root.screen
        visible: !root.floating && (DesktopEditing.active || root.opened || root.dragging || slide.running)
        onVisibleChanged: if (!visible)
            root.useDragSize = false
        color: "transparent"
        contentItem.clip: true
        // Keep the Wayland buffer geometry fixed. Resize only QML content so
        // right-edge anchoring never races compositor configure/commit cycles.
        readonly property real extent: root.edge === "top"
            ? (root.useDragSize ? Math.max(1, root.dragExtent) : Math.round((root.screen ? root.screen.height : 800) * saved.heightFraction))
            : Math.max(root.minimumSideWidth, root.useDragSize ? root.dragExtent : Math.min(root.maxSideWidth, Math.round((root.screen ? root.screen.width : 1280) * saved.widthFraction)))
        implicitWidth: root.screen ? root.screen.width : 1280
        implicitHeight: (root.screen ? root.screen.height : 800) - (root.edge === "top" ? Theme.barHeight : 0)
        exclusiveZone: visible && !root.fullscreenApp ? Math.round(extent * reveal) : 0
        mask: Region {
            x: root.edge === "right" ? Math.round(panel.width - panel.extent * panel.reveal) : 0
            y: 0
            width: root.inputSuspended ? 0 : root.edge === "top" ? panel.width : Math.round(panel.extent * panel.reveal)
            height: root.inputSuspended ? 0 : root.edge === "top" ? Math.round(panel.extent * panel.reveal) : panel.height
        }
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-terminal-sidebar"
        WlrLayershell.keyboardFocus: root.inputSuspended || DesktopEditing.active ? WlrKeyboardFocus.None : root.focusRequested ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.OnDemand
        anchors {
            top: true
            bottom: root.edge !== "top"
            left: root.edge !== "right"
            right: root.edge !== "left"
        }
        margins.top: root.edge === "top" ? Theme.barHeight : 0

        Rectangle {
            id: panelSurface
            readonly property real geometryRevision: panel.reveal
            NativeEditSurface {
                anchors.fill: parent; anchors.topMargin: sidebarHeader.height; geometryItem: panelSurface
                window: panel; zoneName: "sidebar"; vertical: true
                entries: [{id: root.contentType, item: root.activePanel}].concat(root.externalEntries)
            }
            width: root.edge === "top" ? panel.width : panel.extent
            height: root.edge === "top" ? panel.extent : panel.height
            x: root.edge === "right" ? panel.width - width : 0
            clip: true
            color: Theme.barBackground
            radius: 0
            transform: Translate {
                x: root.edge === "left" ? -(1 - panel.reveal) * panelSurface.width : root.edge === "right" ? (1 - panel.reveal) * panelSurface.width : 0
                y: root.edge === "top" ? -(1 - panel.reveal) * panelSurface.height : 0
            }
            Rectangle {
                parent: panel.contentItem
                z: 100
                visible: panelSurface.visible
                color: Theme.panelOuterOutline
                Component.onCompleted: transform = panelSurface.transform
                x: root.edge === "top" ? root.desktopCornerSize : root.edge === "left" ? panelSurface.width : panelSurface.x - 1
                y: root.edge === "top" ? panelSurface.height : root.fullscreenApp ? 0 : Theme.barHeight + root.desktopCornerSize
                width: root.edge === "top" ? Math.max(0, panel.width - root.desktopCornerSize * 2) : 1
                height: root.edge === "top" ? 1 : Math.max(0, panel.height - y)
            }
            Rectangle {
                // Join the desktop corner normally; in fullscreen continue
                // the straight side border to the top of the screen.
                color: Theme.barDivider
                x: root.edge === "top" ? root.desktopCornerSize : root.edge === "left" ? parent.width - 1 : 0
                y: root.edge === "top" ? parent.height - 1 : root.fullscreenApp ? 0 : Theme.barHeight + root.desktopCornerSize
                width: root.edge === "top" ? Math.max(0, parent.width - root.desktopCornerSize * 2) : 1
                height: root.edge === "top" ? 1 : Math.max(0, parent.height - y)
            }
            Item {
                id: sidebarContents
                objectName: "sidebarContents"
                parent: root.floating ? detachedWindow.contentItem : panelSurface
                anchors.fill: parent
                Item {
                    id: sidebarHeader
                    height: Theme.barHeight
                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.right: parent.right
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.gap
                        anchors.rightMargin: Theme.gap
                        spacing: Theme.spaceSmall
                        ActionButton {
                            id: contentPicker
                            objectName: "sidebarContentPicker"
                            WidgetEditHandle { control: contentPicker; widgetId: root.contentType; previewSource: false; onRequested: (id, item) => root.widgetEditRequested(id, item, root.widgetWindow) }
                            text: root.currentContent.label
                            presentation: DesktopLayout.presentation(DesktopEditing.desktop, root.contentType, "sidebar", root.currentContent.label, root.currentContent.icon, true, true)
                            alignLeft: true
                            iconName: root.currentContent.icon
                            Layout.preferredWidth: implicitWidth + Theme.gap
                            Layout.maximumWidth: sidebarHeader.width - Theme.gap * 2
                            Layout.minimumWidth: 32
                            horizontalPadding: Theme.barControlPadding
                            implicitHeight: Theme.barHeight
                            hoverEnabled: true
                            trailingIconName: contentMenu.visible ? "pan-up-symbolic" : "pan-down-symbolic"
                            background: BarControlSurface {
                                hovered: contentPicker.hovered
                                pressed: contentPicker.down
                                selected: contentMenu.visible
                                focused: contentPicker.visualFocus
                            }
                            Accessible.name: "Sidebar content: " + root.currentContent.label
                            onClicked: contentMenu.visible = !contentMenu.visible
                        }
                        Item { Layout.fillWidth: true }
                    }


                }
                GridLayout {
                    id: widgetGrid
                    columns: 1
                    anchors.top: sidebarHeader.bottom
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.margins: Theme.gap
                    anchors.leftMargin: Theme.gap * 2
                    anchors.rightMargin: Theme.gap * 2
                    rowSpacing: Theme.gap
                    ColumnLayout {
                        Layout.row: root.widgetRow(root.contentType)
                        Layout.column: 0
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumHeight: 1
                        spacing: Theme.gap
                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.minimumWidth: 0
                            Layout.minimumHeight: 1
                            clip: true
                            Loader {
                                id: terminalLoader
                                activeFocusOnTab: visible
                                anchors.fill: parent
                                visible: root.contentType === "terminal"
                                active: root.terminalCreated
                                source: "SidebarTerminal.qml"
                                onLoaded: { DesktopEditing.registerSource("terminal", item); if (root.opened && visible) item.focusTerminal(); }
                                onActiveFocusChanged: if (activeFocus && visible && root.terminalReady)
                                    item.focusTerminal()
                            }
                            SidebarNotes {
                                id: notes
                                Component.onCompleted: DesktopEditing.registerSource("notes", notes)
                                menuHost: root.floating ? detachedWindow.contentItem : null
                                screen: root.floating ? detachedWindow.screen : root.screen
                                anchors.fill: parent
                                visible: root.contentType === "notes"
                            }
                            SidebarMonitor {
                                id: monitor
                                Component.onCompleted: DesktopEditing.registerSource("monitor", monitor)
                                anchors.fill: parent
                                visible: root.contentType === "monitor"
                                metrics: root.systemMetrics
                            }
                            Loader {
                                id: extraPanel
                                anchors.fill: parent
                                active: ["calendar", "media", "tasks"].includes(root.contentType)
                                visible: active
                                source: root.contentType === "calendar" ? "SidebarCalendar.qml" : root.contentType === "media" ? "SidebarMedia.qml" : "SidebarTasks.qml"
                                onLoaded: { DesktopEditing.registerSource(root.contentType, item); if (root.opened) item.focusContent(); }
                            }
                        }
                        Text {
                            visible: root.contentType === "terminal" && (terminalLoader.status === Loader.Error || (root.terminalReady && !terminalLoader.item?.shellRunning))
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: terminalLoader.status === Loader.Error ? "Terminal component unavailable. Install QMLTermWidget for Qt 6, then retry." : "Shell exited. Start a new shell to continue."
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                        }
                        ActionButton {
                            visible: root.contentType === "terminal" && (terminalLoader.status === Loader.Error || (root.terminalReady && !terminalLoader.item?.shellRunning))
                            text: terminalLoader.status === Loader.Error ? "Retry" : "New shell"
                            onClicked: root.restartTerminal()
                        }
                    }
                }
            }
        }

        Shortcut {
            sequence: "Ctrl+Shift+F12"
            enabled: panel.visible
            onActivated: root.hide()
        }
    }

    FloatingWindow {
        id: detachedWindow
        objectName: "sidebarDetachedWindow"
        title: root.currentContent.label + " — Bingux"
        screen: root.screen
        visible: root.floating && root.opened
        implicitWidth: 400
        implicitHeight: 640
        minimumSize: Qt.size(180, 240)
        color: Theme.barBackground
        onClosed: {
            if (root.detached) {
                root.hide();
                saved.detached = false;
                saved.setValue("detached", false);
                saved.sync();
            }
        }
        Shortcut {
            sequence: "Ctrl+Shift+F12"
            enabled: detachedWindow.visible
            onActivated: root.hide()
        }
    }

    Variants {
        model: [false, true]
        PanelWindow {
            required property var modelData
            id: cornerWindow
            readonly property bool rightCorner: modelData
            screen: root.screen
            visible: panel.visible && root.desktopCornerSize > 0 && (root.edge === "top" || root.edge === (rightCorner ? "right" : "left"))
            implicitWidth: Math.max(1, root.desktopCornerSize + 1)
            implicitHeight: implicitWidth
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            mask: Region {}
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-sidebar-corner"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors { top: true; left: !rightCorner; right: rightCorner }
            margins.top: Theme.barHeight + root.topInset - 1
            margins.left: Math.max(0, root.leftInset - 1)
            margins.right: Math.max(0, root.rightInset - 1)
            ShellCorner {
                anchors.fill: parent
                mirrored: cornerWindow.rightCorner
                opacity: panel.reveal
            }
        }
    }

    ShellPopup {
        id: contentMenu
        hostItem: root.floating ? detachedWindow.contentItem : null
        cornerRadius: Theme.radius
        surfaceColor: Theme.popupSurface
        screen: root.floating ? detachedWindow.screen : root.screen
        popupWidth: 190
        popupHeight: Math.min(contentMenuColumn.implicitHeight + contentPadding * 2, height - preferredY - Theme.gap)
        contentPadding: Theme.spaceSmall
        property point anchorPoint: Qt.point(0, 0)
        preferredX: anchorPoint.x
        preferredY: anchorPoint.y + Theme.spaceSmall
        onVisibleChanged: if (visible) {
            anchorPoint = root.floating
                ? contentPicker.mapToItem(detachedWindow.contentItem, 0, contentPicker.height)
                : contentPicker.mapToGlobal(0, contentPicker.height);
            Qt.callLater(contentNavigation.focusMenu);
        }
        MenuNavigator {
            id: contentNavigation
            entries: contentMenuColumn.children
            focusTarget: contentMenuColumn
            onEscapeRequested: contentMenu.visible = false
            onActivateRequested: entry => entry.triggered()
            onCurrentEntryChanged: {
                if (!keyboardNavigation || !currentEntry) return;
                if (currentEntry.y < contentMenuScroll.contentY) contentMenuScroll.contentY = currentEntry.y;
                else if (currentEntry.y + currentEntry.height > contentMenuScroll.contentY + contentMenuScroll.height)
                    contentMenuScroll.contentY = currentEntry.y + currentEntry.height - contentMenuScroll.height;
            }
        }
        Flickable {
            id: contentMenuScroll
            anchors.fill: parent
            contentHeight: contentMenuColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            ColumnLayout {
                id: contentMenuColumn
                objectName: "sidebarContentMenu"
                width: parent.width
                spacing: 2
                Keys.forwardTo: [contentNavigation]
                Repeater {
                    model: root.contentTypes
                    ActionButton {
                        id: sidebarWidget
                        required property var modelData
                        objectName: "sidebar-select-" + modelData.id
                        WidgetEditHandle { control: sidebarWidget; widgetId: sidebarWidget.modelData.id; previewSource: false; onRequested: (id, item) => root.widgetEditRequested(id, item, contentMenu) }
                        readonly property bool menuEntry: true
                        signal triggered()
                        text: modelData.label
                        presentation: DesktopLayout.presentation(DesktopEditing.desktop, modelData.id, "sidebar", modelData.label, modelData.icon, true, true)
                        alignLeft: true
                        cornerRadius: contentMenu.contentRadius
                        iconName: modelData.icon
                        flat: root.contentType !== modelData.id
                        hoverEnabled: true
                        onHoveredChanged: if (hovered) contentNavigation.pointerActivate()
                        Layout.fillWidth: true
                        implicitHeight: 32
                        Keys.forwardTo: [contentNavigation]
                        onClicked: triggered()
                        onTriggered: {
                            contentMenu.visible = false;
                            root.selectContent(modelData.id);
                        }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.barDivider; Layout.topMargin: 4; Layout.bottomMargin: 4 }
                ActionButton {
                    readonly property bool menuEntry: true
                    signal triggered()
                    visible: !DesktopEditing.active
                    Layout.fillWidth: true
                    implicitHeight: 32
                    text: root.detached ? "Dock in sidebar" : "Pop out window"
                    iconName: root.detached ? "view-restore-symbolic" : "window-new-symbolic"
                    flat: true
                    alignLeft: true
                    cornerRadius: contentMenu.contentRadius
                    Keys.forwardTo: [contentNavigation]
                    onClicked: triggered()
                    onTriggered: root.detached ? root.dockBack() : root.popOut()
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.barDivider; Layout.topMargin: 4; Layout.bottomMargin: 4 }
                Text { text: "Position"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall; Layout.leftMargin: 8 }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Repeater {
                        model: ["left", "top", "right"]
                        AbstractButton {
                            id: sideButton
                            required property string modelData
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            implicitHeight: 48
                            hoverEnabled: true
                            activeFocusOnTab: true
                            Accessible.name: "Move sidebar to " + modelData
                            Accessible.role: Accessible.Button
                            Accessible.onPressAction: clicked()
                            onClicked: root.setEdge(modelData)
                            Keys.onReturnPressed: clicked()
                            background: BarControlSurface {
                                hovered: sideButton.hovered
                                pressed: sideButton.down
                                selected: root.edge === sideButton.modelData
                                focused: sideButton.visualFocus
                            }
                            contentItem: Item {
                                Text {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    anchors.bottom: parent.bottom
                                    anchors.bottomMargin: 5
                                    text: sideButton.modelData.charAt(0).toUpperCase() + sideButton.modelData.slice(1)
                                    color: root.edge === sideButton.modelData ? Theme.text : Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                }
                                Rectangle {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    y: 8
                                    width: Theme.iconSize
                                    height: Theme.iconSize - 4
                                    radius: 2
                                    color: "transparent"
                                    border.width: 1
                                    border.color: root.edge === sideButton.modelData ? Theme.text : Theme.muted
                                    Rectangle {
                                        x: sideButton.modelData === "right" ? parent.width - width - 2 : 2
                                        y: 2
                                        width: sideButton.modelData === "top" ? parent.width - 4 : 3
                                        height: sideButton.modelData === "top" ? 2 : parent.height - 4
                                        radius: 0.5
                                        color: parent.border.color
                                    }
                                }
                            }
                            ShellTooltip {
                                parent: sideButton
                                visible: sideButton.hovered
                                text: sideButton.Accessible.name
                            }
                        }
                    }
                }
            }
        }
    }

    Timer {
        id: dismiss
        interval: 650
        onTriggered: if (!root.gestureActive && !root.handleHovered) {
            root.handleVisible = false
            root.followingPointer = false
        }
    }

    PanelWindow {
        id: sensor
        screen: root.screen
        visible: !root.detached && !root.inputSuspended && root.settings.sidebarEnabled && (!panel.visible || root.opened || root.gestureActive)
        color: "transparent"
        readonly property int grabWidth: (root.gestureActive ? root.startedOpen : root.opened) ? 24 : root.fullscreenApp ? 2 : Theme.gap
        implicitWidth: grabWidth
        implicitHeight: grabWidth
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-sidebar-edge"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        readonly property real inputOffset: root.gestureActive ? root.sensorGrabOffset : root.opened ? root.handleOffset : 0
        margins.top: root.edge === "top" ? inputOffset : 0
        margins.left: root.edge === "left" ? inputOffset : 0
        margins.right: root.edge === "right" ? inputOffset : 0
        mask: Region {
            x: root.edge === "top" ? root.handleStart : 0
            y: root.edge === "top" ? 0 : root.handleStart
            width: root.edge === "top" ? root.handleEnd - root.handleStart : sensor.width
            height: root.edge === "top" ? sensor.height : root.handleEnd - root.handleStart
        }
        anchors {
            top: true
            bottom: root.edge !== "top"
            left: root.edge !== "right"
            right: root.edge !== "left"
        }
        EdgeGesture {
            id: sensorGesture
            tracksEdge: true
        }
    }

    component GripArtwork: Item {
        anchors.fill: parent
        opacity: handleWindow.reveal * root.boundaryOpacity
        transform: Translate {
            x: root.edge === "top" ? 0 : (root.edge === "left" ? -8 : 8) * (1 - handleWindow.reveal)
            y: root.edge === "top" ? -8 * (1 - handleWindow.reveal) : 0
        }
        Rectangle {
            readonly property bool horizontal: root.edge === "top"
            property real thickness: handleButton.down ? 5 : root.handleHovered ? 4 : 3
            property real length: handleButton.down ? 38 : root.handleHovered ? 48 : 44
            scale: Theme.reducedMotion ? 1 : 1 + root.hintStrength * 0.18
            width: horizontal ? length : thickness
            height: horizontal ? thickness : length
            x: horizontal ? (parent.width - width) / 2 : root.edge === "left" ? 3 : parent.width - width - 3
            y: horizontal ? 3 : (parent.height - height) / 2
            radius: thickness / 2
            color: handleButton.down || dragHint.running ? Theme.accent : Theme.muted
            opacity: (root.handleHovered ? 0.85 : 0.5) + root.hintStrength * (root.handleHovered ? 0.15 : 0.5)
        }
    }

    PanelWindow {
        id: handleWindow
        readonly property bool revealed: root.gestureActive || (root.pillAllowed && (root.opened || (sensor.visible && root.handleVisible)))
        property real reveal: revealed ? 1 : 0
        screen: root.screen
        visible: !root.detached && !root.inputSuspended && (revealed || reveal > 0)
        color: "transparent"
        contentItem.clip: true
        Behavior on reveal {
            NumberAnimation {
                duration: Theme.reducedMotion ? 0 : 160
                easing.type: Easing.OutCubic
            }
        }
        implicitWidth: root.edge === "top" ? 72 : 24
        implicitHeight: root.edge === "top" ? 24 : 72
        // Keep the stationary edge sensor reachable while the pill moves over it.
        // Otherwise each crossing switches to a moving input surface and stops tracking.
        mask: Region {
            x: root.edge === "left" ? sensor.grabWidth : 0
            y: root.edge === "top" ? sensor.grabWidth : 0
            width: handleWindow.width - (root.edge === "top" ? 0 : sensor.grabWidth)
            height: handleWindow.height - (root.edge === "top" ? sensor.grabWidth : 0)
        }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-sidebar-button"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors {
            top: true
            left: root.edge !== "right"
            right: root.edge === "right"
        }
        readonly property real capturePosition: root.gestureActive ? root.handleGrabPosition : root.animatedPosition
        readonly property real baseTop: root.edge === "top" ? 0 : Math.max(root.handleStart, Math.min(capturePosition - height / 2, root.handleEnd - height))
        readonly property real baseLeft: root.edge === "top" ? Math.max(root.handleStart, Math.min(capturePosition - width / 2, root.handleEnd - width)) : 0
        readonly property real inputOffset: root.gestureActive ? root.gestureOffset : root.handleOffset
        margins.top: baseTop + (root.edge === "top" ? inputOffset : 0)
        margins.left: baseLeft + (root.edge === "left" ? inputOffset : 0)
        margins.right: root.edge === "right" ? inputOffset : 0
        EdgeGesture {
            id: handleButton
            enabled: handleWindow.revealed
            Accessible.role: Accessible.Button
            Accessible.name: root.opened ? "Resize sidebar" : "Open sidebar"
            Accessible.description: "Drag inward to open or resize; below " + root.closeThreshold + " pixels closes the sidebar"
            Accessible.onPressAction: root.hintDrag()
        }
        GripArtwork {
            visible: !root.gestureActive
        }
    }

    PanelWindow {
        screen: root.screen
        visible: root.gestureActive && (root.pillAllowed || root.boundaryOpacity > 0)
        color: "transparent"
        mask: Region {}
        implicitWidth: handleWindow.width
        implicitHeight: handleWindow.height
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-sidebar-grip"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        anchors {
            top: true
            left: root.edge !== "right"
            right: root.edge === "right"
        }
        margins.top: root.edge === "top" ? root.handleOffset : Math.max(root.handleStart, Math.min(root.animatedPosition - height / 2, root.handleEnd - height))
        margins.left: root.edge === "top" ? Math.max(root.handleStart, Math.min(root.animatedPosition - width / 2, root.handleEnd - width)) : root.edge === "left" ? root.handleOffset : 0
        margins.right: root.edge === "right" ? root.handleOffset : 0
        GripArtwork {}
    }
}
