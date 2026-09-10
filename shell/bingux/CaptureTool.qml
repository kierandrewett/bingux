import QtQuick
import "CaptureOptions.js" as CaptureOptions
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

Scope {
    id: root
    property var screen
    property bool opened: false
    property bool ready: false
    property var previews: ({})
    property string previewToken: ""
    property int previewRequest: 0
    property bool regionDragging: false
    property Item previewItem: null
    property var capabilities: ({})
    property var captureWindows: []
    property var selectedWindow: null
    readonly property bool directWindowPicker: !!capabilities.windowPicker && preferences.backend === "auto"
    function windowAt(x, y) {
        return captureWindows.find(window => x >= window.x && y >= window.y
            && x < window.x + window.width && y < window.y + window.height) || null;
    }
    function hoverWindow(screen, x, y) {
        if (preferences.target !== "window" || !directWindowPicker) return;
        activeScreen = screen;
        selectedWindow = windowAt(screen.x + x, screen.y + y);
    }
    function cycleWindow(direction) {
        if (!captureWindows.length) return;
        const index = captureWindows.indexOf(selectedWindow);
        selectedWindow = captureWindows[(index + direction + captureWindows.length) % captureWindows.length];
        activeScreen = Quickshell.screens.find(screen => selectedWindow.x < screen.x + screen.width
            && selectedWindow.x + selectedWindow.width > screen.x && selectedWindow.y < screen.y + screen.height
            && selectedWindow.y + selectedWindow.height > screen.y) || activeScreen;
    }
    property var activeScreen: screen || Quickshell.screens[0]
    property alias region: captureSession.region
    property alias hasRegion: captureSession.hasRegion
    property string state: "idle"
    property string message: ""
    property string savedPath: ""
    property string partialPath: ""
    property bool optionsOpen: false
    property double startedAt: 0
    property int elapsed: 0
    property int countdown: 0
    readonly property bool recording: state === "recording"
    readonly property bool busy: ["countdown", "starting", "recording", "finalizing"].includes(state)
    readonly property string elapsedText: Math.floor(elapsed / 60).toString() + ":" + (elapsed % 60).toString().padStart(2, "0")
    signal opening

    PersistentProperties {
        id: captureSession
        reloadableId: "capture-selector-session"
        property bool requested: false
        property rect region: Qt.rect(200, 200, 800, 500)
        property bool hasRegion: false
        onLoaded: if (requested) Qt.callLater(() => root.open())
    }

    Settings {
        id: preferences
        location: Quickshell.env("BINGUX_CAPTURE_SETTINGS_PATH") || "file://" + (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/capture.ini"
        category: "capture"
        property string kind: "screenshot"
        property string target: "region"
        property bool cursor: false
        property bool copy: true
        property int delay: 0
        property int fps: 30
        property int maxHeight: 1080
        property string quality: "balanced"
        property string audio: "none"
        property string format: "png"
        property string directory: ""
        property string encoder: "auto"
        property string backend: "auto"
    }

    function optionsSnapshot() {
        return {kind: preferences.kind, target: preferences.target, cursor: preferences.cursor,
            copy: preferences.copy, delay: preferences.delay, fps: preferences.fps,
            maxHeight: preferences.maxHeight, quality: preferences.quality, audio: preferences.audio,
            format: preferences.format, directory: preferences.directory, encoder: preferences.encoder,
            backend: preferences.backend, region: {x: Math.round(region.x), y: Math.round(region.y),
                width: Math.round(region.width), height: Math.round(region.height)}};
    }
    function configureOptions(raw) {
        if (busy) return {ok: false, error: "Cannot change options during capture"};
        let options;
        try { options = CaptureOptions.validate(JSON.parse(raw), activeScreen); }
        catch (error) { return {ok: false, error: String(error.message || error)}; }
        if (options.target === "window" && ready && !capabilities.window)
            return {ok: false, error: "Window capture is unavailable"};
        for (const key of Object.keys(options)) {
            if (key === "region") {
                const area = options.region;
                region = Qt.rect(area.x, area.y, area.width, area.height);
                hasRegion = true;
            } else preferences[key] = options[key];
        }
        preferences.sync();
        return {ok: true, options: optionsSnapshot()};
    }

    function send(record) { worker.write(JSON.stringify(record) + "\n"); }
    function open() {
        if (!worker.running) worker.running = true;
        if (recording) { stop(); return; }
        if (busy) { send({command: "cancel"}); return; }
        if (opened || state === "preparing") { close(); return; }
        feedback.visible = false;
        captureSession.requested = true;
        message = "";
        selectedWindow = null;
        captureWindows = [];
        state = "preparing";
        const nextScreen = screen || Quickshell.screens[0];
        const changedScreen = activeScreen !== nextScreen;
        activeScreen = nextScreen;
        if (!hasRegion || changedScreen) resetRegion();
        if (ready) preparePreview();
    }
    function preparePreview() {
        previewRequest++;
        send({command: "preview", request: previewRequest, screens: Quickshell.screens.map(s => ({name: s.name, x: s.x, y: s.y, width: s.width, height: s.height}))});
    }
    function resetRegion() {
        if (!activeScreen) return;
        hasRegion = true;
        region = Qt.rect(Math.round(activeScreen.width * .2), Math.round(activeScreen.height * .2), Math.round(activeScreen.width * .6), Math.round(activeScreen.height * .5));
    }
    function close() {
        captureSession.requested = false;
        opened = false; optionsOpen = false; regionDragging = false;
        previewRequest++;
        if (!busy) { state = "idle"; send({command: "cancel"}); }
    }
    function stop() { send({command: "stop"}); }
    function take() {
        if (!ready || busy) return;
        if (preferences.target === "window" && (!capabilities.window || (directWindowPicker && !selectedWindow))) return;
        const s = activeScreen;
        const windowId = preferences.target === "window" && directWindowPicker ? selectedWindow.id : undefined;
        captureSession.requested = false;
        opened = false;
        state = "countdown";
        optionsOpen = false;
        send({command: "capture", kind: preferences.kind, target: preferences.target, screen: s.name,
            previewToken: root.previewToken,
            windowId: windowId,
            region: {x: Math.round(s.x + region.x), y: Math.round(s.y + region.y), width: Math.round(region.width), height: Math.round(region.height)},
            cursor: preferences.cursor, copy: preferences.copy, delay: preferences.delay, fps: preferences.fps,
            maxHeight: preferences.maxHeight, quality: preferences.quality, audio: preferences.audio,
            format: preferences.format, directory: preferences.directory, encoder: preferences.encoder, backend: preferences.backend});
    }
    function handle(event) {
        if (event.event === "ready") {
            capabilities = event; ready = true;
            if (state === "preparing") preparePreview();
        } else if (event.event === "preview") {
            if (state !== "preparing" || event.request !== previewRequest) return;
            captureWindows = event.windows || [];
            previews = event.images;
            previewToken = event.token;
            state = "selecting";
            // Dismiss/focus other shell surfaces only AFTER their pixels are frozen.
            opening();
            opened = true;
            if (event.warning) message = "Live preview · " + event.warning;
        } else if (event.event === "countdown") {
            state = "countdown"; countdown = event.seconds;
        } else if (event.event === "starting") {
            state = "starting";
        } else if (event.event === "recording") {
            feedback.visible = false;
            state = "recording"; startedAt = Number.isFinite(event.started) ? event.started * 1000 : Date.now();
            elapsed = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
        } else if (event.event === "finalizing") {
            state = "finalizing";
        } else if (event.event === "saved") {
            state = "saved"; savedPath = event.path; partialPath = "";
            message = (event.kind === "recording" ? "Recording saved" : "Screenshot saved") + (event.copied ? " · Copied to clipboard" : "");
            feedback.visible = false;
            if (!event.notified) console.warn("Capture saved; desktop notification acknowledgement pending or unavailable");
        } else if (event.event === "error") {
            captureSession.requested = false;
            state = "error"; message = event.message; partialPath = event.partial || "";
            feedback.visible = true;
        } else if (event.event === "cancelled" && state !== "preparing") { state = "idle"; }
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_CAPTURE_HELPER") ? [Quickshell.env("BINGUX_CAPTURE_HELPER")]
            : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("capture_service.py").toString().replace(/^file:\/\//, ""))]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: data => {
                try { root.handle(JSON.parse(data)); }
                catch (error) { console.warn("Capture response:", error); }
            }
        }
        onExited: {
            const wasActive = root.opened || root.busy;
            root.ready = false;
            if (root.state !== "error") {
                root.state = "error";
                root.message = "Capture connection stopped. Open Capture to reconnect to the recorder.";
            }
            if (wasActive) feedback.visible = true;
        }
    }
    IpcHandler {
        target: "capture"
        function options(): string { return JSON.stringify(root.optionsSnapshot()); }
        function configure(raw: string): string { return JSON.stringify(root.configureOptions(raw)); }
        function openOptions(raw: string): string {
            const result = root.configureOptions(raw);
            if (!result.ok) return JSON.stringify(result);
            if (!root.opened && root.state !== "preparing") root.open();
            return JSON.stringify(result);
        }
        function open(): void { root.open(); }
        function show(kind: string, target: string): string {
            if (kind && !["screenshot", "recording"].includes(kind))
                return JSON.stringify({ok: false, error: "Unknown capture mode"});
            if (target && !["region", "screen", "window"].includes(target))
                return JSON.stringify({ok: false, error: "Unknown capture target"});
            if (root.busy) return JSON.stringify({ok: false, error: "A capture is already in progress"});
            if (target === "window" && root.ready && !root.capabilities.window)
                return JSON.stringify({ok: false, error: "Window capture is unavailable"});
            if (kind) preferences.kind = kind;
            if (target) preferences.target = target;
            if (!root.opened && root.state !== "preparing") root.open();
            return JSON.stringify({ok: true});
        }
        function take(): string {
            if (!root.opened || !root.ready || root.busy)
                return JSON.stringify({ok: false, error: "Open the capture selector before taking a capture"});
            if (preferences.target === "window" && !root.capabilities.window)
                return JSON.stringify({ok: false, error: "Window capture is unavailable"});
            if (preferences.target === "window" && root.directWindowPicker && !root.selectedWindow)
                return JSON.stringify({ok: false, error: "Select a window first"});
            root.take();
            return JSON.stringify({ok: true});
        }
        function stop(): void { root.stop(); }
        function cancel(): void { if (root.busy) root.send({command: "cancel"}); else root.close(); }
        function status(): string { return JSON.stringify({state: root.state, ready: root.ready, opened: root.opened, mode: preferences.kind, target: preferences.target, elapsed: root.elapsed, savedPath: root.savedPath, message: root.message, requested: captureSession.requested, selectedWindow: root.selectedWindow, windows: root.captureWindows, region: root.region, dragging: root.regionDragging, screen: root.activeScreen ? {width: root.activeScreen.width, height: root.activeScreen.height} : null, capabilities: root.capabilities}); }
    }
    Timer {
        interval: 1000; running: root.busy; repeat: true
        onTriggered: {
            if (root.recording) root.elapsed = Math.floor((Date.now() - root.startedAt) / 1000);
            else if (root.countdown > 0) root.countdown--;
        }
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: overlay
            required property var modelData
            screen: modelData
            visible: root.opened
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-capture"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors { top: true; bottom: true; left: true; right: true }
            readonly property bool active: root.activeScreen === modelData
            // Pointer coordinates end at extent - 1; selection bounds are exclusive.
            function edgeCoordinate(value, extent) {
                return value <= 1 ? 0 : value >= extent - 1 ? extent : value;
            }
            readonly property rect selection: active && preferences.target === "region" ? root.region : Qt.rect(0, 0, width, height)
            Image { anchors.fill: parent; source: root.previews[overlay.modelData.name]?.plain || ""; fillMode: Image.Stretch; cache: false }
            Image { anchors.fill: parent; source: root.previews[overlay.modelData.name]?.cursor || ""; fillMode: Image.Stretch; cache: false; visible: preferences.cursor }
            Shortcut { sequence: "Escape"; context: Qt.ApplicationShortcut; enabled: root.opened && overlay.active; onActivated: root.close() }
            Shortcut {
                sequences: ["Return", "Enter"]
                context: Qt.ApplicationShortcut
                autoRepeat: false
                enabled: root.opened && overlay.active && !root.optionsOpen && !root.regionDragging
                onActivated: root.take()
            }
            Shortcut {
                sequence: "Tab"
                context: Qt.ApplicationShortcut
                enabled: root.opened && overlay.active && preferences.target === "window" && root.directWindowPicker && !root.optionsOpen
                onActivated: root.cycleWindow(1)
            }
            Shortcut {
                sequence: "Shift+Tab"
                context: Qt.ApplicationShortcut
                enabled: root.opened && overlay.active && preferences.target === "window" && root.directWindowPicker && !root.optionsOpen
                onActivated: root.cycleWindow(-1)
            }
            Rectangle { anchors.fill: parent; color: "#99000000"; visible: !overlay.active || preferences.target === "window" }
            Rectangle { x: 0; y: 0; width: parent.width; height: overlay.selection.y; color: "#88000000"; visible: overlay.active && preferences.target === "region" }
            Rectangle { x: 0; y: overlay.selection.y; width: overlay.selection.x; height: overlay.selection.height; color: "#88000000"; visible: overlay.active && preferences.target === "region" }
            Rectangle { x: overlay.selection.x + overlay.selection.width; y: overlay.selection.y; width: parent.width - x; height: overlay.selection.height; color: "#88000000"; visible: overlay.active && preferences.target === "region" }
            Rectangle { x: 0; y: overlay.selection.y + overlay.selection.height; width: parent.width; height: parent.height - y; color: "#88000000"; visible: overlay.active && preferences.target === "region" }

            MouseArea {
                id: selectionDrag
                objectName: "captureRegionDraw"
                anchors.fill: parent
                preventStealing: true
                hoverEnabled: preferences.target === "window" && root.directWindowPicker
                onEntered: root.hoverWindow(overlay.modelData, mouseX, mouseY)
                onClicked: mouse => {
                    if (preferences.target !== "window" || !root.directWindowPicker) return;
                    root.hoverWindow(overlay.modelData, mouse.x, mouse.y);
                    if (root.selectedWindow) root.take();
                }
                cursorShape: preferences.target === "region" ? Qt.CrossCursor : Qt.ArrowCursor
                property point origin
                onPressed: mouse => {
                    root.regionDragging = preferences.target === "region";
                    root.activeScreen = overlay.modelData;
                    origin = Qt.point(overlay.edgeCoordinate(mouse.x, overlay.width), overlay.edgeCoordinate(mouse.y, overlay.height));
                    if (preferences.target === "region") root.region = Qt.rect(Math.min(origin.x, overlay.width - 2), Math.min(origin.y, overlay.height - 2), 2, 2);
                }
                onReleased: root.regionDragging = false
                onCanceled: root.regionDragging = false
                onPositionChanged: mouse => {
                    root.hoverWindow(overlay.modelData, mouse.x, mouse.y);
                    if (!pressed || preferences.target !== "region") return;
                    const x = overlay.edgeCoordinate(mouse.x, overlay.width);
                    const y = overlay.edgeCoordinate(mouse.y, overlay.height);
                    root.region = Qt.rect(Math.min(origin.x, x, overlay.width - 2), Math.min(origin.y, y, overlay.height - 2), Math.max(2, Math.abs(x - origin.x)), Math.max(2, Math.abs(y - origin.y)));
                }
            }
            Rectangle {
                objectName: "captureWindowHighlight"
                readonly property var window: root.selectedWindow
                visible: preferences.target === "window" && root.directWindowPicker && window !== null
                x: window ? Math.max(0, window.x - overlay.modelData.x) : 0
                y: window ? Math.max(0, window.y - overlay.modelData.y) : 0
                width: window ? Math.max(0, Math.min(overlay.width, window.x + window.width - overlay.modelData.x) - x) : 0
                height: window ? Math.max(0, Math.min(overlay.height, window.y + window.height - overlay.modelData.y) - y) : 0
                color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.3)
                border.color: Theme.accent
                border.width: 3
                radius: 8
                Column {
                    anchors.centerIn: parent
                    width: Math.max(0, Math.min(360, parent.width - 32))
                    spacing: 12
                    OsIconImage {
                        anchors.horizontalCenter: parent.horizontalCenter
                        implicitSize: 64
                        readonly property var entry: root.selectedWindow ? (DesktopEntries.byId(root.selectedWindow.appId)
                            || DesktopEntries.byId(root.selectedWindow.appId.replace(/\.desktop$/, ""))) : null
                        source: Quickshell.iconPath(entry?.icon || "application-x-executable", "application-x-executable")
                    }
                    Text {
                        width: parent.width
                        text: root.selectedWindow?.title || root.selectedWindow?.appName || ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        horizontalAlignment: Text.AlignHCenter
                        color: "white"
                        style: Text.Outline
                        styleColor: "#80000000"
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        font.weight: Font.Medium
                    }
                }
            }
            Rectangle {
                id: selectionBorder
                visible: overlay.active && preferences.target !== "window"
                x: preferences.target === "screen" ? 4 : root.region.x
                y: preferences.target === "screen" ? 4 : root.region.y
                width: preferences.target === "screen" ? overlay.width - 8 : root.region.width
                height: preferences.target === "screen" ? overlay.height - 8 : root.region.height
                color: "transparent"
                border.color: "white"
                border.width: 1
                radius: preferences.target === "screen" ? 12 : 2
                MouseArea {
                    objectName: "captureRegionMove"
                    anchors.fill: parent
                    preventStealing: true
                    enabled: preferences.target === "region"
                    cursorShape: Qt.SizeAllCursor
                    property point origin
                    property rect initial
                    onPressed: mouse => { root.regionDragging = true; origin = mapToItem(overlay.contentItem, mouse.x, mouse.y); initial = root.region; }
                    onReleased: root.regionDragging = false
                    onCanceled: root.regionDragging = false
                    onPositionChanged: mouse => {
                        if (!pressed) return;
                        const p = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                        const x = p.x <= 1 ? 0 : p.x >= overlay.width - 1 ? overlay.width - initial.width : initial.x + p.x - origin.x;
                        const y = p.y <= 1 ? 0 : p.y >= overlay.height - 1 ? overlay.height - initial.height : initial.y + p.y - origin.y;
                        root.region = Qt.rect(Math.max(0, Math.min(overlay.width - initial.width, x)), Math.max(0, Math.min(overlay.height - initial.height, y)), initial.width, initial.height);
                    }
                }
                Repeater {
                    model: preferences.target === "region" ? [
                        {x: 0, y: 0}, {x: 1, y: 0}, {x: 0, y: 1}, {x: 1, y: 1},
                        {x: .5, y: 0}, {x: .5, y: 1}, {x: 0, y: .5}, {x: 1, y: .5}
                    ] : []
                    Rectangle {
                        required property var modelData
                        width: modelData.x === .5 ? 24 : modelData.y === .5 ? 4 : 8
                        height: modelData.y === .5 ? 24 : modelData.x === .5 ? 4 : 8
                        radius: 4; color: "white"
                        x: modelData.x * selectionBorder.width - width / 2
                        y: modelData.y * selectionBorder.height - height / 2
                        MouseArea {
                            objectName: "captureRegionResize" + modelData.x + "_" + modelData.y
                            anchors.fill: parent; anchors.margins: -8
                            preventStealing: true
                            cursorShape: modelData.x === .5 ? Qt.SizeVerCursor : modelData.y === .5 ? Qt.SizeHorCursor : modelData.x === modelData.y ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor
                            property rect initial
                            onPressed: { initial = root.region; root.regionDragging = true; }
                            onReleased: root.regionDragging = false
                            onCanceled: root.regionDragging = false
                            onPositionChanged: mouse => {
                                if (!pressed) return;
                                const position = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                                const p = Qt.point(overlay.edgeCoordinate(position.x, overlay.width), overlay.edgeCoordinate(position.y, overlay.height));
                                const right = initial.x + initial.width, bottom = initial.y + initial.height;
                                const left = modelData.x === 0 ? Math.max(0, Math.min(right - 2, p.x)) : initial.x;
                                const top = modelData.y === 0 ? Math.max(0, Math.min(bottom - 2, p.y)) : initial.y;
                                root.region = Qt.rect(left, top, modelData.x === 1 ? Math.max(2, Math.min(overlay.width, p.x) - left) : right - left, modelData.y === 1 ? Math.max(2, Math.min(overlay.height, p.y) - top) : bottom - top);
                            }
                        }
                    }
                }
                Rectangle {
                    visible: preferences.target === "region"
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: parent.y < 40 ? 12 : -36
                    width: sizeLabel.implicitWidth + Theme.padding * 2; height: 28; radius: Theme.radius; color: Theme.popupSurface
                    Text { id: sizeLabel; anchors.centerIn: parent; text: Math.round(root.region.width) + " × " + Math.round(root.region.height); color: "white"; font.family: Theme.fontFamily; font.pixelSize: 12 }
                }
            }
            Text {
                visible: overlay.active && root.message !== ""
                anchors.horizontalCenter: parent.horizontalCenter
                y: Theme.barHeight + Theme.padding
                width: Math.min(640, overlay.width - 32)
                text: root.message
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            PanelWindow {
                id: controls
                screen: overlay.screen
                visible: overlay.visible && overlay.active
                onVisibleChanged: if (visible) root.previewItem = controls.contentItem
                color: "transparent"
                exclusionMode: ExclusionMode.Ignore
                anchors { top: true; bottom: true; left: true; right: true }
                WlrLayershell.layer: WlrLayer.Overlay
                WlrLayershell.namespace: "bingux-capture-controls"
                WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
                mask: Region {
                    item: toolbar
                    Region {
                        x: optionsPanel.x; y: optionsPanel.y
                        width: optionsPanel.visible ? optionsPanel.width : 0
                        height: optionsPanel.visible ? optionsPanel.height : 0
                    }
                }
                // The preview is a separate buffer below this surface, so the
                // compositor can blur it behind the translucent controls.
                BlurRegion {
                    window: controls
                    surfaceNamespace: "bingux-capture-controls"
                    region: optionsPanel.visible
                        ? Qt.rect(Math.min(toolbar.x, optionsPanel.x), Math.min(toolbar.y, optionsPanel.y),
                            Math.max(toolbar.x + toolbar.width, optionsPanel.x + optionsPanel.width) - Math.min(toolbar.x, optionsPanel.x),
                            Math.max(toolbar.y + toolbar.height, optionsPanel.y + optionsPanel.height) - Math.min(toolbar.y, optionsPanel.y))
                        : Qt.rect(toolbar.x, toolbar.y, toolbar.width, toolbar.height)
                }
                Item {
                    anchors.fill: parent
                    focus: overlay.visible && overlay.active
                    Keys.onEscapePressed: root.close()
                    Keys.onPressed: event => {
                        const delta = event.modifiers & Qt.ShiftModifier ? 10 : 1;
                        if (preferences.target === "region" && [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) {
                            const dx = event.key === Qt.Key_Left ? -delta : event.key === Qt.Key_Right ? delta : 0;
                            const dy = event.key === Qt.Key_Up ? -delta : event.key === Qt.Key_Down ? delta : 0;
                            root.region = Qt.rect(Math.max(0, Math.min(overlay.width - root.region.width, root.region.x + dx)), Math.max(0, Math.min(overlay.height - root.region.height, root.region.y + dy)), root.region.width, root.region.height);
                            event.accepted = true;
                        }
                    }
                }

                Rectangle {
                    id: toolbar
                    objectName: "captureToolbar"
                    visible: overlay.active && root.opened
                    property real interactionOpacity: root.regionDragging ? .25 : 1
                    opacity: interactionOpacity
                    Behavior on interactionOpacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 100; easing.type: Easing.OutCubic } }
                    property real movedX: -1
                    property real movedY: -1
                    x: movedX < 0 ? (overlay.width - width) / 2 : Math.max(8, Math.min(overlay.width - width - 8, movedX))
                    y: movedY < 0 ? overlay.height - height - Theme.padding * 2 : Math.max(8, Math.min(overlay.height - height - 8, movedY))
                    width: toolbarRow.implicitWidth + 16
                    height: 56
                    radius: Theme.cardRadius
                    color: Theme.popupSurface
                    border.color: Theme.outline
                    MouseArea { anchors.fill: parent } // Controls must not start a region drag.
                    RowLayout {
                        id: toolbarRow
                        anchors.centerIn: parent
                        spacing: Theme.gap
                        Item {
                            Layout.preferredWidth: 20
                            Layout.preferredHeight: 40
                            Grid {
                                anchors.centerIn: parent
                                columns: 2; spacing: 4
                                Repeater { model: 6; Rectangle { width: 3; height: 3; radius: 1.5; color: dragHandle.containsMouse ? Theme.text : Theme.muted } }
                            }
                            MouseArea {
                                id: dragHandle
                                objectName: "captureToolbarHandle"
                                anchors.fill: parent
                                preventStealing: true
                                hoverEnabled: true
                                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                                property point origin
                                property point start
                                onPressed: mouse => { origin = mapToItem(controls.contentItem, mouse.x, mouse.y); start = Qt.point(toolbar.x, toolbar.y); }
                                onPositionChanged: mouse => {
                                    if (!pressed) return;
                                    const point = mapToItem(controls.contentItem, mouse.x, mouse.y);
                                    toolbar.movedX = start.x + point.x - origin.x;
                                    toolbar.movedY = start.y + point.y - origin.y;
                                }
                            }
                            ShellTooltip { visible: dragHandle.containsMouse && !dragHandle.pressed; text: "Drag to move toolbar" }
                        }
                        SegmentedControl {
                            implicitWidth: 84
                            implicitHeight: 44
                            options: ["screenshot", "recording"]
                            currentValue: preferences.kind
                            accessiblePrefix: "Capture mode: "
                            onSelected: value => preferences.kind = value
                            segmentContent: Component {
                                Item {
                                    SymbolicIcon { anchors.centerIn: parent; implicitSize: 18; source: Quickshell.iconPath(parent.parent.segmentValue === "screenshot" ? "camera-photo-symbolic" : "camera-video-symbolic"); color: parent.parent.segmentSelected ? Theme.text : Theme.muted }
                                }
                            }
                        }
                        SegmentedControl {
                            implicitWidth: overlay.width < 720 ? 128 : 320
                            implicitHeight: 44
                            options: ["region", "window", "screen"]
                            currentValue: preferences.target
                            disabledOptions: root.ready && !root.capabilities.window ? ["window"] : []
                            accessiblePrefix: "Capture "
                            onSelected: value => preferences.target = value
                            segmentContent: Component {
                                Item {
                                    id: targetContent
                                    readonly property string value: parent.segmentValue
                                    RowLayout {
                                        anchors.centerIn: parent
                                        spacing: Theme.gap
                                        SymbolicIcon { implicitSize: 18; source: Quickshell.iconPath(targetContent.value === "region" ? "screenshot-selection-symbolic" : targetContent.value === "window" ? "screenshot-window-symbolic" : "video-display-symbolic", "video-display-symbolic"); color: targetContent.parent.segmentSelected ? Theme.text : Theme.muted }
                                        Text { visible: overlay.width >= 720; text: targetContent.value.charAt(0).toUpperCase() + targetContent.value.slice(1); color: targetContent.parent.segmentSelected ? Theme.text : Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                                    }
                                }
                            }
                        }
                        CaptureButton { objectName: "captureCursorToggle"; iconName: "input-mouse-symbolic"; chosen: preferences.cursor; description: preferences.cursor ? "Mouse cursor included" : "Mouse cursor hidden"; onClicked: preferences.cursor = !preferences.cursor }
                        CaptureButton { objectName: "captureSettingsToggle"; iconName: "preferences-system-symbolic"; description: "Capture settings"; chosen: root.optionsOpen; onClicked: root.optionsOpen = !root.optionsOpen }
                        CaptureButton {
                            text: preferences.kind === "recording" ? "Record" : "Capture"
                            iconName: preferences.kind === "recording" ? "media-record-symbolic" : ""
                            description: !root.ready ? "Preparing capture…" : preferences.target === "window" ? (root.directWindowPicker ? "Capture selected window (Enter)" : "Choose window and capture") : "Capture (Enter)"
                            primary: true
                            enabled: root.ready && (preferences.target !== "window" || (!!root.capabilities.window && (!root.directWindowPicker || root.selectedWindow !== null)))
                            onClicked: root.take()
                        }
                        CaptureButton { iconName: "window-close-symbolic"; description: "Cancel (Esc)"; onClicked: root.close() }
                    }
                }
                Rectangle {
                    id: optionsPanel
                    visible: overlay.active && root.optionsOpen
                    width: Math.min(440, overlay.width - 32)
                    height: Math.min(optionsColumn.implicitHeight + 32, overlay.height - toolbar.height - 48)
                    x: Math.min(overlay.width - width - 16, toolbar.x + toolbar.width - width)
                    y: toolbar.y > height + 20 ? toolbar.y - height - 12 : Math.min(overlay.height - height - 8, toolbar.y + toolbar.height + 12)
                    radius: Theme.cardRadius
                    color: Theme.popupSurface
                    border.color: Theme.outline
                    MouseArea { anchors.fill: parent }
                    Flickable {
                        anchors.fill: parent
                        anchors.margins: 16
                        contentHeight: optionsColumn.implicitHeight
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar {}
                    ColumnLayout {
                        id: optionsColumn
                        width: parent.width
                        spacing: 12
                        RowLayout {
                            Layout.fillWidth: true; spacing: 8
                            Text { text: "Capture settings"; font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.DemiBold; color: Theme.text }
                            Item { Layout.fillWidth: true }
                            CaptureButton { iconName: "window-close-symbolic"; compact: true; description: "Close settings"; onClicked: root.optionsOpen = false }
                        }
                        GridLayout {
                            Layout.fillWidth: true; columns: 2; columnSpacing: 12; rowSpacing: 12
                            CaptureChoice { label: "Quality"; choices: ["Compact", "Balanced", "High"]; values: ["compact", "balanced", "high"]; value: preferences.quality; onChosen: value => preferences.quality = value }
                            CaptureChoice { label: "Delay"; choices: ["None", "3 seconds", "5 seconds", "10 seconds"]; values: [0, 3, 5, 10]; value: preferences.delay; onChosen: value => preferences.delay = value }
                            CaptureChoice { visible: preferences.kind === "recording"; label: "Frame rate"; choices: ["15 fps", "30 fps", "60 fps"]; values: [15, 30, 60]; value: preferences.fps; onChosen: value => preferences.fps = value }
                            CaptureChoice { visible: preferences.kind === "recording"; label: "Resolution limit"; choices: ["720p", "1080p", "1440p", "2160p", "Original"]; values: [720, 1080, 1440, 2160, 0]; value: preferences.maxHeight; onChosen: value => preferences.maxHeight = value }
                            CaptureChoice { visible: preferences.kind === "recording"; label: "Audio"; choices: ["None", "System audio", "Microphone", "Both"]; values: ["none", "system", "microphone", "both"]; value: preferences.audio; enabled: !!root.capabilities.audio; onChosen: value => preferences.audio = value }
                            CaptureChoice { visible: preferences.kind === "recording"; label: "Encoding"; choices: ["Automatic (CPU fallback)", "Software / CPU"]; values: ["auto", "cpu"]; value: preferences.encoder; onChosen: value => preferences.encoder = value }
                            CaptureChoice { visible: preferences.kind === "screenshot"; label: "Image format"; choices: ["PNG · lossless", "JPEG · smaller"]; values: ["png", "jpeg"]; value: preferences.format; onChosen: value => preferences.format = value }
                            CaptureChoice { label: "Capture backend"; choices: ["Automatic", "Desktop portal"]; values: ["auto", "portal"]; value: preferences.backend; onChosen: value => preferences.backend = value }
                            ColumnLayout {
                                Layout.columnSpan: 2; Layout.fillWidth: true
                                Text { text: "Save folder"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                TextField {
                                    selectionColor: Theme.textSelection
                                    selectedTextColor: Theme.text
                                    Layout.fillWidth: true
                                    text: preferences.directory
                                    placeholderText: "Default: Pictures/Screenshots or Videos/Recordings"
                                    color: Theme.text; placeholderTextColor: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 13
                                    onEditingFinished: preferences.directory = text.trim()
                                    padding: Theme.gap
                                    background: Rectangle { color: Theme.elevated; radius: Theme.radius; border.color: parent.activeFocus ? Theme.accent : "transparent" }
                                }
                            }
                        }
                        CaptureButton { visible: preferences.kind === "screenshot"; text: "Copy to clipboard"; iconName: "edit-copy-symbolic"; chosen: preferences.copy; onClicked: preferences.copy = !preferences.copy }
                    }
                    }
                }
            }
        }
    }

    PanelWindow {
        id: feedback
        visible: false
        screen: root.screen
        anchors { bottom: true; right: true }
        margins { bottom: 112; right: 24 }
        implicitWidth: 420
        implicitHeight: feedbackContents.implicitHeight + 32
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-capture-feedback"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        Rectangle { anchors.fill: parent; color: Theme.popupSurface; radius: 20; border.color: Theme.outline }
        ColumnLayout {
            id: feedbackContents
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16 }
            spacing: 10
            Text { Layout.fillWidth: true; text: root.message; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.DemiBold }
            Text { visible: root.partialPath !== ""; Layout.fillWidth: true; text: "Partial recording preserved: " + root.partialPath; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
            RowLayout {
                Item { Layout.fillWidth: true }
                CaptureButton { text: "Dismiss"; onClicked: feedback.visible = false }
            }
        }
    }

    component CaptureButton: ActionButton {
        id: button
        property string description: text
        property bool chosen: false
        property bool primary: false
        property bool compact: false
        implicitHeight: compact ? Theme.controlHeight : 36
        implicitWidth: text === "" ? implicitHeight : contents.implicitWidth + Theme.padding * 2
        padding: 0
        horizontalPadding: 0
        hoverEnabled: true
        Accessible.name: description
        background: Rectangle {
            radius: Theme.radius - Theme.spaceSmall
            color: button.primary ? (button.down ? Qt.darker(Theme.accent, 1.1) : button.hovered ? Qt.lighter(Theme.accent, 1.08) : Theme.accent) : button.down ? Theme.pressed : button.chosen ? Theme.hover : button.hovered ? Theme.elevated : "transparent"
            border.color: button.activeFocus ? Theme.accent : "transparent"
            border.width: button.visualFocus ? 2 : 0
            opacity: button.enabled ? 1 : .4
        }
        contentItem: RowLayout {
            id: contents
            spacing: 8
            Item { Layout.fillWidth: true }
            SymbolicIcon { visible: button.iconName !== ""; implicitSize: button.compact ? 14 : 18; source: Quickshell.iconPath(button.iconName, "application-x-executable-symbolic"); color: button.primary ? "#17212c" : button.enabled ? Theme.text : Theme.muted }
            Text { visible: button.text !== ""; text: button.text; color: button.primary ? "#17212c" : button.enabled ? Theme.text : Theme.muted; font.family: Theme.fontFamily; font.pixelSize: button.compact ? 12 : 13; font.weight: button.primary || button.chosen ? Font.DemiBold : Font.Normal }
            Item { Layout.fillWidth: true }
        }
        ShellTooltip { visible: (button.hovered || button.visualFocus) && button.description !== ""; text: button.description }
    }
    component CaptureChoice: ColumnLayout {
        id: choice
        property string label
        property var choices
        property var values
        property var value
        readonly property alias popup: choiceMenu
        signal chosen(var value)
        Layout.fillWidth: true
        spacing: 5
        Text { text: choice.label; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
        ActionButton {
            id: choiceButton
            objectName: "captureChoice" + choice.label
            Layout.fillWidth: true
            text: choice.choices[Math.max(0, choice.values.indexOf(choice.value))]
            Accessible.name: choice.label + ": " + text
            onClicked: choiceMenu.visible = !choiceMenu.visible
            contentItem: RowLayout {
                spacing: Theme.gap
                Text { Layout.fillWidth: true; Layout.leftMargin: Theme.gap; text: choiceButton.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                SymbolicIcon { Layout.rightMargin: Theme.gap; implicitSize: 14; source: Quickshell.iconPath("pan-down-symbolic") }
            }
        }
        ShellPopup {
            id: choiceMenu
            hostItem: choice.Window.window ? choice.Window.window.contentItem : null
            popupWidth: choiceButton.width
            popupHeight: choiceEntries.contentHeight + contentPadding * 2
            contentPadding: Theme.gap
            onAboutToOpen: {
                if (!hostItem) return;
                const anchor = choiceButton.mapToItem(hostItem, 0, 0);
                preferredX = anchor.x;
                const below = anchor.y + choiceButton.height + Theme.spaceSmall;
                preferredY = below + popupHeight <= hostItem.height - Theme.gap
                    ? below : anchor.y - popupHeight - Theme.spaceSmall;
            }
            onVisibleChanged: if (visible) Qt.callLater(() => choiceNavigation.focusMenu())
            Connections {
                target: root
                function onOptionsOpenChanged() { if (!root.optionsOpen) choiceMenu.visible = false; }
                function onOpenedChanged() { if (!root.opened) choiceMenu.visible = false; }
            }
            MenuNavigator {
                id: choiceNavigation
                entries: choice.choices.map((text, index) => ({text, index, enabled: true}))
                view: choiceEntries
                focusTarget: choiceEntries
                onEscapeRequested: choiceMenu.visible = false
                onActivateRequested: entry => { choice.chosen(choice.values[entry.index]); choiceMenu.visible = false; choiceButton.forceActiveFocus(); }
            }
            ListView {
                id: choiceEntries
                objectName: "captureChoiceMenu" + choice.label
                anchors.fill: parent
                model: choice.choices
                spacing: 2
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Keys.onDownPressed: choiceNavigation.move(1)
                Keys.onUpPressed: choiceNavigation.move(-1)
                Keys.onReturnPressed: choiceNavigation.activateCurrent()
                Keys.onSpacePressed: choiceNavigation.activateCurrent()
                Keys.onEscapePressed: choiceMenu.visible = false
                delegate: ActionButton {
                    id: optionButton
                    required property string modelData
                    required property int index
                    width: choiceEntries.width
                    height: 38
                    text: modelData
                    flat: true
                    cornerRadius: choiceMenu.contentRadius
                    onClicked: { choice.chosen(choice.values[index]); choiceMenu.visible = false; choiceButton.forceActiveFocus(); }
                    background: Rectangle { radius: optionButton.cornerRadius; color: optionButton.down ? Theme.pressed : optionButton.hovered || (choiceNavigation.keyboardNavigation && choiceEntries.currentIndex === optionButton.index) ? Theme.hover : "transparent" }
                    contentItem: RowLayout {
                        spacing: Theme.gap
                        SymbolicIcon { Layout.leftMargin: Theme.gap; implicitSize: 16; opacity: choice.values[optionButton.index] === choice.value ? 1 : 0; source: Quickshell.iconPath("object-select-symbolic"); color: Theme.accent }
                        Text { Layout.fillWidth: true; text: optionButton.text; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
                    }
                }
            }
        }
    }
}
