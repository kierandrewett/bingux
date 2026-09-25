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
    property Item selectionItem: null
    property var capabilities: ({})
    property var captureWindows: []
    property var selectedWindow: null
    readonly property bool directWindowPicker: !!capabilities.windowPicker && preferences.backend === "auto"
    function windowAt(x, y) {
        return captureWindows.find(window => x >= window.x && y >= window.y && x < window.x + window.width && y < window.y + window.height) || null;
    }
    function hoverWindow(screen, x, y) {
        if (preferences.target !== "window" || !directWindowPicker)
            return;
        activeScreen = screen;
        selectedWindow = windowAt(screen.x + x, screen.y + y);
    }
    function cycleWindow(direction) {
        if (!captureWindows.length)
            return;
        const index = captureWindows.indexOf(selectedWindow);
        selectedWindow = captureWindows[(index + direction + captureWindows.length) % captureWindows.length];
        activeScreen = Quickshell.screens.find(screen => selectedWindow.x < screen.x + screen.width && selectedWindow.x + selectedWindow.width > screen.x && selectedWindow.y < screen.y + screen.height && selectedWindow.y + selectedWindow.height > screen.y) || activeScreen;
    }
    property var activeScreen: screen || Quickshell.screens[0]
    property alias region: captureSession.region
    property alias hasRegion: captureSession.hasRegion
    property string state: "idle"
    property string message: ""
    property string savedPath: ""
    property string partialPath: ""
    property bool optionsOpen: false
    property bool pickingFolder: false
    property double startedAt: 0
    property int elapsed: 0
    property int countdown: 0
    property rect recordingRegion: Qt.rect(0, 0, 0, 0)
    property string recordingTarget: ""
    readonly property bool recording: state === "recording"
    readonly property bool busy: ["countdown", "starting", "recording", "finalizing"].includes(state)
    readonly property string elapsedText: Math.floor(elapsed / 60).toString() + ":" + (elapsed % 60).toString().padStart(2, "0")
    signal opening
    property alias settings: preferences

    PersistentProperties {
        id: captureSession
        reloadableId: "capture-selector-session"
        property bool requested: false
        property rect region: Qt.rect(200, 200, 800, 500)
        property bool hasRegion: false
        property string lastNotifiedError: ""
        onLoaded: if (requested)
            Qt.callLater(() => root.open())
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
        return {
            kind: preferences.kind,
            target: preferences.target,
            cursor: preferences.cursor,
            copy: preferences.copy,
            delay: preferences.delay,
            fps: preferences.fps,
            maxHeight: preferences.maxHeight,
            quality: preferences.quality,
            audio: preferences.audio,
            format: preferences.format,
            directory: preferences.directory,
            encoder: preferences.encoder,
            backend: preferences.backend,
            region: {
                x: Math.round(region.x),
                y: Math.round(region.y),
                width: Math.round(region.width),
                height: Math.round(region.height)
            }
        };
    }
    function configureOptions(raw) {
        if (busy)
            return {
                ok: false,
                error: "Cannot change options during capture"
            };
        let options;
        try {
            options = CaptureOptions.validate(JSON.parse(raw), activeScreen);
        } catch (error) {
            return {
                ok: false,
                error: String(error.message || error)
            };
        }
        if (options.target === "window" && ready && !capabilities.window)
            return {
                ok: false,
                error: "Window capture is unavailable"
            };
        for (const key of Object.keys(options)) {
            if (key === "region") {
                const area = options.region;
                region = Qt.rect(area.x, area.y, area.width, area.height);
                hasRegion = true;
            } else
                preferences[key] = options[key];
        }
        preferences.sync();
        return {
            ok: true,
            options: optionsSnapshot()
        };
    }

    function send(record) {
        worker.write(JSON.stringify(record) + "\n");
    }
    function open() {
        if (!worker.running)
            worker.running = true;
        if (recording) {
            stop();
            return;
        }
        if (busy) {
            send({
                command: "cancel"
            });
            return;
        }
        if (opened || state === "preparing") {
            close();
            return;
        }
        captureSession.lastNotifiedError = "";
        captureSession.requested = true;
        message = "";
        selectedWindow = null;
        captureWindows = [];
        state = "preparing";
        const nextScreen = screen || Quickshell.screens[0];
        const changedScreen = activeScreen !== nextScreen;
        activeScreen = nextScreen;
        if (!hasRegion || changedScreen)
            resetRegion();
        if (ready)
            preparePreview();
    }
    function preparePreview() {
        previewRequest++;
        send({
            command: "preview",
            request: previewRequest,
            screens: Quickshell.screens.map(s => ({
                        name: s.name,
                        x: s.x,
                        y: s.y,
                        width: s.width,
                        height: s.height
                    }))
        });
    }
    function resetRegion() {
        if (!activeScreen)
            return;
        hasRegion = true;
        region = Qt.rect(Math.round(activeScreen.width * .2), Math.round(activeScreen.height * .2), Math.round(activeScreen.width * .6), Math.round(activeScreen.height * .5));
    }
    function backOrClose() {
        if (optionsOpen)
            optionsOpen = false;
        else
            close();
    }
    function notifyError() {
        const detail = message + (partialPath ? "\nPartial recording preserved: " + partialPath : "");
        if (!detail || detail === captureSession.lastNotifiedError)
            return;
        captureSession.lastNotifiedError = detail;
        ShellNotifications.send("Capture failed", detail);
    }
    function close() {
        captureSession.requested = false;
        opened = false;
        optionsOpen = false;
        regionDragging = false;
        previewRequest++;
        if (!busy) {
            state = "idle";
            send({
                command: "cancel"
            });
        }
    }
    function stop(hoveredAt = 0) {
        send({
            command: "stop",
            hoveredAt: hoveredAt
        });
    }
    function take() {
        if (!ready || busy)
            return;
        if (preferences.target === "window" && (!capabilities.window || (directWindowPicker && !selectedWindow)))
            return;
        const s = activeScreen;
        const windowId = preferences.target === "window" && directWindowPicker ? selectedWindow.id : undefined;
        captureSession.requested = false;
        opened = false;
        state = "countdown";
        optionsOpen = false;
        send({
            command: "capture",
            kind: preferences.kind,
            target: preferences.target,
            screen: s.name,
            previewToken: root.previewToken,
            windowId: windowId,
            region: {
                x: Math.round(s.x + region.x),
                y: Math.round(s.y + region.y),
                width: Math.round(region.width),
                height: Math.round(region.height)
            },
            cursor: preferences.cursor,
            copy: preferences.copy,
            delay: preferences.delay,
            fps: preferences.fps,
            maxHeight: preferences.maxHeight,
            quality: preferences.quality,
            audio: preferences.audio,
            format: preferences.format,
            directory: preferences.directory,
            encoder: preferences.encoder,
            backend: preferences.backend
        });
    }
    function handle(event) {
        if (event.event === "ready") {
            capabilities = event;
            ready = true;
            if (state === "preparing")
                preparePreview();
        } else if (event.event === "preview") {
            if (state !== "preparing" || event.request !== previewRequest)
                return;
            captureWindows = event.windows || [];
            previews = event.images;
            previewToken = event.token;
            state = "selecting";
            // Dismiss/focus other shell surfaces only AFTER their pixels are frozen.
            opening();
            opened = true;
            if (event.warning)
                message = "Live preview · " + event.warning;
        } else if (event.event === "countdown") {
            state = "countdown";
            countdown = event.seconds;
        } else if (event.event === "starting") {
            state = "starting";
        } else if (event.event === "recording") {
            captureSession.lastNotifiedError = "";
            state = "recording";
            recordingTarget = event.target || "";
            if (event.region)
                recordingRegion = Qt.rect(event.region.x, event.region.y, event.region.width, event.region.height);
            startedAt = Number.isFinite(event.started) ? event.started * 1000 : Date.now();
            elapsed = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
        } else if (event.event === "finalizing") {
            state = "finalizing";
        } else if (event.event === "saved") {
            state = "saved";
            savedPath = event.path;
            partialPath = "";
            message = (event.kind === "recording" ? "Recording saved" : "Screenshot saved") + (event.copied ? " · Copied to clipboard" : "");
            if (!event.notified)
                console.warn("Capture saved; desktop notification acknowledgement pending or unavailable");
        } else if (event.event === "error") {
            captureSession.requested = false;
            state = "error";
            message = event.message;
            partialPath = event.partial || "";
            notifyError();
        } else if (event.event === "cancelled" && state !== "preparing") {
            state = "idle";
        }
    }
    Process {
        id: worker
        command: Quickshell.env("BINGUX_CAPTURE_HELPER") ? [Quickshell.env("BINGUX_CAPTURE_HELPER")] : ["python3", "-u", decodeURIComponent(Qt.resolvedUrl("capture_service.py").toString().replace(/^file:\/\//, ""))]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: data => {
                try {
                    root.handle(JSON.parse(data));
                } catch (error) {
                    console.warn("Capture response:", error);
                }
            }
        }
        onExited: {
            const wasActive = root.opened || root.busy;
            root.ready = false;
            if (root.state !== "error") {
                root.state = "error";
                root.message = "Capture connection stopped. Open Capture to reconnect to the recorder.";
            }
            if (wasActive)
                root.notifyError();
        }
    }
    IpcHandler {
        target: "capture"
        function options(): string {
            return JSON.stringify(root.optionsSnapshot());
        }
        function configure(raw: string): string {
            return JSON.stringify(root.configureOptions(raw));
        }
        function openOptions(raw: string): string {
            const result = root.configureOptions(raw);
            if (!result.ok)
                return JSON.stringify(result);
            if (!root.opened && root.state !== "preparing")
                root.open();
            return JSON.stringify(result);
        }
        function open(): void {
            root.open();
        }
        function show(kind: string, target: string): string {
            if (kind && !["screenshot", "recording"].includes(kind))
                return JSON.stringify({
                    ok: false,
                    error: "Unknown capture mode"
                });
            if (target && !["region", "screen", "window"].includes(target))
                return JSON.stringify({
                    ok: false,
                    error: "Unknown capture target"
                });
            if (root.busy)
                return JSON.stringify({
                    ok: false,
                    error: "A capture is already in progress"
                });
            if (target === "window" && root.ready && !root.capabilities.window)
                return JSON.stringify({
                    ok: false,
                    error: "Window capture is unavailable"
                });
            if (kind)
                preferences.kind = kind;
            if (target)
                preferences.target = target;
            if (!root.opened && root.state !== "preparing")
                root.open();
            return JSON.stringify({
                ok: true
            });
        }
        function take(): string {
            if (!root.opened || !root.ready || root.busy)
                return JSON.stringify({
                    ok: false,
                    error: "Open the capture selector before taking a capture"
                });
            if (preferences.target === "window" && !root.capabilities.window)
                return JSON.stringify({
                    ok: false,
                    error: "Window capture is unavailable"
                });
            if (preferences.target === "window" && root.directWindowPicker && !root.selectedWindow)
                return JSON.stringify({
                    ok: false,
                    error: "Select a window first"
                });
            root.take();
            return JSON.stringify({
                ok: true
            });
        }
        function stop(): void {
            root.stop();
        }
        function cancel(): void {
            if (root.busy)
                root.send({
                    command: "cancel"
                });
            else
                root.close();
        }
        function status(): string {
            return JSON.stringify({
                state: root.state,
                ready: root.ready,
                opened: root.opened,
                mode: preferences.kind,
                target: preferences.target,
                elapsed: root.elapsed,
                savedPath: root.savedPath,
                message: root.message,
                requested: captureSession.requested,
                selectedWindow: root.selectedWindow,
                windows: root.captureWindows,
                region: root.region,
                dragging: root.regionDragging,
                screen: root.activeScreen ? {
                    width: root.activeScreen.width,
                    height: root.activeScreen.height
                } : null,
                capabilities: root.capabilities
            });
        }
    }
    Timer {
        interval: 1000
        running: root.busy
        repeat: true
        onTriggered: {
            if (root.recording)
                root.elapsed = Math.floor((Date.now() - root.startedAt) / 1000);
            else if (root.countdown > 0)
                root.countdown--;
        }
    }

    RecordingRegion {
        active: root.recording && root.recordingTarget === "region"
        region: root.recordingRegion
    }

    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: overlay
            required property var modelData
            screen: modelData
            visible: root.opened && !root.pickingFolder
            onVisibleChanged: if (visible && active)
                root.selectionItem = overlay.contentItem
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-capture"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            readonly property bool active: root.activeScreen === modelData
            // Pointer coordinates end at extent - 1; selection bounds are exclusive.
            function edgeCoordinate(value, extent) {
                return value <= 1 ? 0 : value >= extent - 1 ? extent : value;
            }
            readonly property rect selection: active && preferences.target === "region" ? root.region : Qt.rect(0, 0, width, height)
            Image {
                anchors.fill: parent
                source: root.previews[overlay.modelData.name]?.plain || ""
                visible: preferences.kind === "screenshot"
                fillMode: Image.Stretch
                cache: false
            }
            Image {
                anchors.fill: parent
                source: root.previews[overlay.modelData.name]?.cursor || ""
                fillMode: Image.Stretch
                cache: false
                visible: preferences.kind === "screenshot" && preferences.cursor
            }
            Shortcut {
                sequence: "Escape"
                context: Qt.ApplicationShortcut
                autoRepeat: false
                enabled: root.opened && overlay.active && !root.pickingFolder
                onActivated: root.backOrClose()
            }
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
            Rectangle {
                anchors.fill: parent
                color: "#99000000"
                visible: !overlay.active || preferences.target === "window"
            }
            Rectangle {
                x: 0
                y: 0
                width: parent.width
                height: overlay.selection.y
                color: "#88000000"
                visible: overlay.active && preferences.target === "region"
            }
            Rectangle {
                x: 0
                y: overlay.selection.y
                width: overlay.selection.x
                height: overlay.selection.height
                color: "#88000000"
                visible: overlay.active && preferences.target === "region"
            }
            Rectangle {
                x: overlay.selection.x + overlay.selection.width
                y: overlay.selection.y
                width: parent.width - x
                height: overlay.selection.height
                color: "#88000000"
                visible: overlay.active && preferences.target === "region"
            }
            Rectangle {
                x: 0
                y: overlay.selection.y + overlay.selection.height
                width: parent.width
                height: parent.height - y
                color: "#88000000"
                visible: overlay.active && preferences.target === "region"
            }

            MouseArea {
                id: selectionDrag
                objectName: "captureRegionDraw"
                anchors.fill: parent
                preventStealing: true
                hoverEnabled: preferences.target === "window" && root.directWindowPicker
                onEntered: root.hoverWindow(overlay.modelData, mouseX, mouseY)
                onClicked: mouse => {
                    if (preferences.target !== "window" || !root.directWindowPicker)
                        return;
                    root.hoverWindow(overlay.modelData, mouse.x, mouse.y);
                    if (root.selectedWindow)
                        root.take();
                }
                cursorShape: preferences.target === "region" ? Qt.CrossCursor : Qt.ArrowCursor
                property point origin
                onPressed: mouse => {
                    root.regionDragging = preferences.target === "region";
                    root.activeScreen = overlay.modelData;
                    origin = Qt.point(overlay.edgeCoordinate(mouse.x, overlay.width), overlay.edgeCoordinate(mouse.y, overlay.height));
                    if (preferences.target === "region")
                        root.region = Qt.rect(Math.min(origin.x, overlay.width - 2), Math.min(origin.y, overlay.height - 2), 2, 2);
                }
                onReleased: root.regionDragging = false
                onCanceled: root.regionDragging = false
                onPositionChanged: mouse => {
                    root.hoverWindow(overlay.modelData, mouse.x, mouse.y);
                    if (!pressed || preferences.target !== "region")
                        return;
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
                        readonly property var entry: root.selectedWindow ? (DesktopEntries.byId(root.selectedWindow.appId) || DesktopEntries.byId(root.selectedWindow.appId.replace(/\.desktop$/, ""))) : null
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
                    cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    property point origin
                    property rect initial
                    onPressed: mouse => {
                        root.regionDragging = true;
                        origin = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                        initial = root.region;
                    }
                    onReleased: root.regionDragging = false
                    onCanceled: root.regionDragging = false
                    onPositionChanged: mouse => {
                        if (!pressed)
                            return;
                        const p = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                        const x = p.x <= 1 ? 0 : p.x >= overlay.width - 1 ? overlay.width - initial.width : initial.x + p.x - origin.x;
                        const y = p.y <= 1 ? 0 : p.y >= overlay.height - 1 ? overlay.height - initial.height : initial.y + p.y - origin.y;
                        root.region = Qt.rect(Math.max(0, Math.min(overlay.width - initial.width, x)), Math.max(0, Math.min(overlay.height - initial.height, y)), initial.width, initial.height);
                    }
                }
                Repeater {
                    model: preferences.target === "region" ? [
                        {
                            x: 0,
                            y: 0
                        },
                        {
                            x: 1,
                            y: 0
                        },
                        {
                            x: 0,
                            y: 1
                        },
                        {
                            x: 1,
                            y: 1
                        },
                        {
                            x: .5,
                            y: 0
                        },
                        {
                            x: .5,
                            y: 1
                        },
                        {
                            x: 0,
                            y: .5
                        },
                        {
                            x: 1,
                            y: .5
                        }
                    ] : []
                    Item {
                        id: resizeHandle
                        required property var modelData
                        readonly property bool horizontalEdge: modelData.x === .5
                        readonly property bool verticalEdge: modelData.y === .5
                        readonly property bool corner: !horizontalEdge && !verticalEdge
                        readonly property bool active: resizeMouse.containsMouse || resizeMouse.pressed
                        width: horizontalEdge ? Math.max(0, selectionBorder.width - 32) : 32
                        height: verticalEdge ? Math.max(0, selectionBorder.height - 32) : 32
                        x: modelData.x * selectionBorder.width - width / 2
                        y: modelData.y * selectionBorder.height - height / 2
                        z: corner ? 2 : 1
                        // Edge hit targets cover the whole line, not only its handle.
                        Rectangle {
                            anchors.centerIn: parent
                            width: resizeHandle.horizontalEdge ? parent.width : 2
                            height: resizeHandle.verticalEdge ? parent.height : 2
                            visible: !resizeHandle.corner
                            color: Theme.accent
                            opacity: resizeHandle.active ? 1 : 0
                            Behavior on opacity {
                                NumberAnimation {
                                    duration: Theme.reducedMotion ? 0 : 100
                                }
                            }
                        }
                        CaptureRegionMarker {
                            anchors.centerIn: parent
                            horizontalEdge: resizeHandle.horizontalEdge
                            verticalEdge: resizeHandle.verticalEdge
                            active: resizeHandle.active
                        }
                        MouseArea {
                            id: resizeMouse
                            objectName: "captureRegionResize" + modelData.x + "_" + modelData.y
                            anchors.fill: parent
                            hoverEnabled: true
                            preventStealing: true
                            cursorShape: resizeHandle.horizontalEdge ? Qt.SizeVerCursor : resizeHandle.verticalEdge ? Qt.SizeHorCursor : modelData.x === modelData.y ? Qt.SizeFDiagCursor : Qt.SizeBDiagCursor
                            property rect initial
                            property point origin
                            onPressed: mouse => {
                                initial = root.region;
                                origin = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                                root.regionDragging = true;
                            }
                            onReleased: root.regionDragging = false
                            onCanceled: root.regionDragging = false
                            onPositionChanged: mouse => {
                                if (!pressed)
                                    return;
                                const position = mapToItem(overlay.contentItem, mouse.x, mouse.y);
                                const dx = position.x - origin.x, dy = position.y - origin.y;
                                const right = initial.x + initial.width, bottom = initial.y + initial.height;
                                const edgeX = modelData.x === 0 ? initial.x + dx : right + dx;
                                const edgeY = modelData.y === 0 ? initial.y + dy : bottom + dy;
                                const px = position.x <= 1 ? 0 : position.x >= overlay.width - 1 ? overlay.width : edgeX;
                                const py = position.y <= 1 ? 0 : position.y >= overlay.height - 1 ? overlay.height : edgeY;
                                const left = modelData.x === 0 ? Math.max(0, Math.min(right - 2, px)) : initial.x;
                                const top = modelData.y === 0 ? Math.max(0, Math.min(bottom - 2, py)) : initial.y;
                                root.region = Qt.rect(left, top, modelData.x === 1 ? Math.max(2, Math.min(overlay.width, px) - left) : right - left, modelData.y === 1 ? Math.max(2, Math.min(overlay.height, py) - top) : bottom - top);
                            }
                        }
                    }
                }
                Rectangle {
                    visible: preferences.target === "region"
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: parent.y < 40 ? 12 : -36
                    width: sizeLabel.implicitWidth + Theme.padding * 2
                    height: 28
                    radius: Theme.radius
                    color: Theme.popupSurface
                    Text {
                        id: sizeLabel
                        anchors.centerIn: parent
                        text: Math.round(root.region.width) + " × " + Math.round(root.region.height)
                        color: "white"
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                    }
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
                onVisibleChanged: if (visible)
                    root.previewItem = controls.contentItem
                color: "transparent"
                // The compositor needs the transparent buffer to sample the
                // frozen preview behind the toolbar and settings panel.
                surfaceFormat.opaque: false
                exclusionMode: ExclusionMode.Ignore
                anchors {
                    top: true
                    bottom: true
                    left: true
                    right: true
                }
                WlrLayershell.layer: WlrLayer.Overlay
                WlrLayershell.namespace: "bingux-capture-controls"
                WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
                mask: Region {
                    item: toolbar
                    Region {
                        item: closeCapture
                    }
                }
                // The preview is a separate buffer below this surface, so the
                // compositor can blur it behind the translucent controls.
                BackgroundEffect {
                    target: toolbar
                    radius: toolbar.radius
                }
                BlurRegion {
                    window: controls
                    surfaceNamespace: "bingux-capture-controls"
                    region: Qt.rect(toolbar.x, toolbar.y, toolbar.width, toolbar.height)
                }
                Item {
                    anchors.fill: parent
                    focus: overlay.visible && overlay.active
                    Keys.onEscapePressed: event => {
                        if (!event.isAutoRepeat)
                            root.backOrClose();
                        event.accepted = true;
                    }
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
                    SurfaceFade {
                        target: toolbar
                    }
                    Behavior on interactionOpacity {
                        NumberAnimation {
                            duration: Theme.reducedMotion ? 0 : 100
                            easing.type: Easing.OutCubic
                        }
                    }
                    property real movedX: -1
                    property real movedY: -1
                    readonly property real compactX: movedX < 0 ? (overlay.width - 320) / 2 : Math.max(24, Math.min(overlay.width - 344, movedX))
                    readonly property real compactY: movedY < 0 ? overlay.height - 200 : Math.max(24, Math.min(overlay.height - 200, movedY))
                    readonly property real expandedX: movedX < 0 ? (overlay.width - 360) / 2 : Math.max(24, Math.min(overlay.width - 384, movedX))
                    readonly property real expandedY: movedY < 0 ? overlay.height - settingsHeight - 24 : Math.max(24, Math.min(overlay.height - settingsHeight - 24, movedY))
                    x: compactX + (expandedX - compactX) * resizeProgress
                    y: compactY + (expandedY - compactY) * resizeProgress
                    property real settingsProgress: root.optionsOpen ? 1 : 0
                    property real resizeProgress: root.optionsOpen ? 1 : 0
                    property real settingsHeight: Math.min(optionsPanel.implicitHeight, overlay.height - 64)
                    width: 320 + 40 * resizeProgress
                    height: 176 + (settingsHeight - 176) * resizeProgress
                    clip: true
                    Behavior on settingsProgress {
                        NumberAnimation { duration: Theme.reducedMotion ? 0 : 160; easing.type: Easing.OutCubic }
                    }
                    Behavior on resizeProgress {
                        NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
                    }
                    Behavior on settingsHeight {
                        NumberAnimation { duration: Theme.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
                    }
                    radius: Theme.cardRadius
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.outline
                    PanelOutline {
                        surface: toolbar
                    }
                    MouseArea {
                        id: dragHandle
                        objectName: "captureToolbarHandle"
                        anchors.fill: parent
                        preventStealing: true
                        cursorShape: pressed ? Qt.ClosedHandCursor : Qt.ArrowCursor
                        property point origin
                        property point start
                        onPressed: mouse => {
                            origin = mapToItem(controls.contentItem, mouse.x, mouse.y);
                            start = Qt.point(toolbar.x, toolbar.y);
                        }
                        onPositionChanged: mouse => {
                            if (!pressed)
                                return;
                            const point = mapToItem(controls.contentItem, mouse.x, mouse.y);
                            toolbar.movedX = start.x + point.x - origin.x;
                            toolbar.movedY = start.y + point.y - origin.y;
                        }
                    }
                    CaptureButton {
                        id: closeCapture
                        parent: controls.contentItem
                        x: toolbar.x + toolbar.width - width / 2
                        y: toolbar.y - height / 2
                        z: 1
                        visible: toolbar.visible
                        opacity: toolbar.opacity
                        width: 32
                        height: 32
                        iconSource: Qt.resolvedUrl("icons/capture-close.svg")
                        description: "Cancel (Esc)"
                        background: Rectangle {
                            id: closeSurface
                            radius: width / 2
                            color: closeCapture.down ? "#424242" : closeCapture.hovered ? "#363636" : "#242424"
                            border.width: 1
                            border.color: closeCapture.visualFocus ? Theme.accent : Theme.outline
                            PanelOutline {
                                surface: closeSurface
                            }
                        }
                        onClicked: root.close()
                    }
                    ColumnLayout {
                        visible: opacity > 0
                        enabled: !root.optionsOpen
                        // Hand off between pages without ghosted controls.
                        opacity: Math.max(0, 1 - toolbar.settingsProgress * 2)
                        width: 288
                        height: 144
                        x: toolbar.compactX - toolbar.x + 16 - toolbar.settingsProgress * 12
                        y: toolbar.compactY - toolbar.y + 16
                        spacing: 16
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 80
                            Layout.minimumHeight: 80
                            Layout.maximumHeight: 80
                            spacing: 8
                            Repeater {
                                model: ["region", "screen", "window"]
                                AbstractButton {
                                    id: targetButton
                                    required property string modelData
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 0
                                    Layout.fillHeight: true
                                    hoverEnabled: true
                                    checkable: true
                                    checked: preferences.target === modelData
                                    enabled: modelData !== "window" || !root.ready || !!root.capabilities.window
                                    objectName: "captureTarget" + modelData
                                    Accessible.name: modelData === "region" ? "Selection" : modelData === "screen" ? "Screen" : "Window"
                                    onClicked: preferences.target = modelData
                                    background: ControlCentreButtonSurface {
                                        control: targetButton
                                        radius: 12
                                        baseColor: targetButton.checked ? Theme.hover : "transparent"
                                    }
                                    contentItem: ColumnLayout {
                                        spacing: 8
                                        Item {
                                            Layout.fillHeight: true
                                        }
                                        SymbolicIcon {
                                            Layout.alignment: Qt.AlignHCenter
                                            implicitSize: 24
                                            source: Qt.resolvedUrl("icons/capture-" + (targetButton.modelData === "region" ? "selection" : targetButton.modelData) + ".svg")
                                            color: targetButton.enabled ? Theme.text : Theme.muted
                                        }
                                        Text {
                                            Layout.alignment: Qt.AlignHCenter
                                            text: targetButton.Accessible.name
                                            color: targetButton.enabled ? Theme.text : Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontSmall
                                        }
                                        Item {
                                            Layout.fillHeight: true
                                        }
                                    }
                                }
                            }
                        }
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 48
                            Layout.minimumHeight: 48
                            Layout.maximumHeight: 48
                            Rectangle {
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                width: 76
                                height: 32
                                radius: 16
                                color: Theme.elevated
                                Row {
                                    anchors.fill: parent
                                    anchors.margins: 3
                                    Repeater {
                                        model: ["screenshot", "recording"]
                                        AbstractButton {
                                            id: modeButton
                                            required property string modelData
                                            width: 35
                                            height: 26
                                            hoverEnabled: true
                                            checkable: true
                                            checked: preferences.kind === modelData
                                            objectName: "captureMode" + modelData
                                            Accessible.name: modelData === "screenshot" ? "Screenshot" : "Recording"
                                            onClicked: preferences.kind = modelData
                                            background: Rectangle {
                                                radius: 13
                                                color: modeButton.checked ? Theme.text : modeButton.hovered ? Theme.hover : "transparent"
                                                border.width: modeButton.visualFocus ? 2 : 0
                                                border.color: Theme.accent
                                            }
                                            contentItem: Item {
                                                SymbolicIcon {
                                                    anchors.centerIn: parent
                                                    implicitSize: 16
                                                    source: Qt.resolvedUrl("icons/capture-" + (modeButton.modelData === "screenshot" ? "camera" : "video") + ".svg")
                                                    color: modeButton.checked ? Theme.background : Theme.text
                                                }
                                            }
                                            ShellTooltip {
                                                visible: modeButton.hovered || modeButton.visualFocus
                                                text: modeButton.Accessible.name
                                            }
                                        }
                                    }
                                }
                            }
                            AbstractButton {
                                id: shutter
                                objectName: "captureShutter"
                                anchors.centerIn: parent
                                width: 48
                                height: 48
                                hoverEnabled: true
                                Accessible.name: preferences.kind === "recording" ? "Start recording" : "Take screenshot"
                                enabled: root.ready && (preferences.target !== "window" || (!!root.capabilities.window && (!root.directWindowPicker || root.selectedWindow !== null)))
                                onClicked: root.take()
                                background: Rectangle {
                                    radius: width / 2
                                    color: "transparent"
                                    border.width: 2
                                    border.color: shutter.visualFocus ? Theme.accent : Theme.text
                                    opacity: shutter.enabled ? 1 : 0.4
                                    Rectangle {
                                        anchors.fill: parent
                                        anchors.margins: 5
                                        radius: width / 2
                                        color: preferences.kind === "recording" ? Theme.recordingIndicator : Theme.text
                                        opacity: shutter.down ? 0.65 : shutter.hovered ? 0.85 : 1
                                    }
                                }
                                ShellTooltip {
                                    visible: shutter.hovered || shutter.visualFocus
                                    text: !root.ready ? "Preparing capture…" : shutter.Accessible.name + " (Enter)"
                                }
                            }
                            RowLayout {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 4
                                CaptureButton {
                                    objectName: "captureCursorToggle"
                                    iconSource: Qt.resolvedUrl("icons/capture-cursor.svg")
                                    chosen: preferences.cursor
                                    description: preferences.cursor ? "Mouse cursor included" : "Mouse cursor hidden"
                                    onClicked: preferences.cursor = !preferences.cursor
                                }
                                CaptureButton {
                                    objectName: "captureSettingsToggle"
                                    iconSource: Qt.resolvedUrl("icons/capture-settings.svg")
                                    description: "Capture settings"
                                    chosen: root.optionsOpen
                                    onClicked: root.optionsOpen = !root.optionsOpen
                                }
                            }
                        }
                    }
                }
                CaptureSettings {
                    id: optionsPanel
                    parent: toolbar
                    width: 360
                    // Keep the page laid out at its destination. The card
                    // reveals it; its rows must not ride the moving top edge.
                    height: toolbar.settingsHeight
                    x: toolbar.expandedX - toolbar.x + (1 - toolbar.settingsProgress) * 12
                    y: toolbar.expandedY - toolbar.y
                    visible: opacity > 0
                    enabled: root.optionsOpen
                    opacity: Math.max(0, toolbar.settingsProgress * 2 - 1)
                    preferences: root.settings
                    audioAvailable: !!root.capabilities.audio
                    onPickingFolderChanged: root.pickingFolder = pickingFolder
                    onBack: root.optionsOpen = false
                }
            }
        }
    }

    component CaptureButton: ActionButton {
        id: button
        property string description: text
        property url iconSource: ""
        property bool chosen: false
        property bool primary: false
        property bool compact: false
        implicitHeight: compact ? Theme.controlHeight : 36
        implicitWidth: text === "" ? implicitHeight : contents.implicitWidth + Theme.padding * 2
        padding: 0
        horizontalPadding: 0
        hoverEnabled: true
        Accessible.name: description
        background: ControlCentreButtonSurface {
            control: button
            radius: Theme.radius - Theme.spaceSmall
            baseColor: button.primary ? Theme.accent : button.chosen ? Theme.hover : "transparent"
            opacity: button.enabled ? 1 : .4
        }
        contentItem: RowLayout {
            id: contents
            spacing: 8
            Item {
                Layout.fillWidth: true
            }
            SymbolicIcon {
                visible: button.iconName !== "" || button.iconSource.toString() !== ""
                implicitSize: button.compact ? 14 : 18
                source: button.iconSource.toString() !== "" ? button.iconSource : Quickshell.iconPath(button.iconName, "application-x-executable-symbolic")
                color: button.primary ? "#17212c" : button.enabled ? Theme.text : Theme.muted
            }
            Text {
                visible: button.text !== ""
                text: button.text
                color: button.primary ? "#17212c" : button.enabled ? Theme.text : Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: button.compact ? 12 : 13
                font.weight: button.primary || button.chosen ? Font.DemiBold : Font.Normal
            }
            Item {
                Layout.fillWidth: true
            }
        }
        ShellTooltip {
            visible: (button.hovered || button.visualFocus) && button.description !== ""
            text: button.description
        }
    }
}
