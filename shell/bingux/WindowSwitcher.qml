import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets

Scope {
    id: root
    readonly property alias nativeWindow: window
    property bool enabled: true
    property int showDelay: 0
    property bool active: false
    property bool shown: false
    property double shownAt: 0
    property var activeStreams: []
    property var notifications: []
    property var previews: ({})
    property bool previewPending: false
    property var previewTimes: ({})
    property var previewAttempts: ({})
    readonly property int previewCacheAge: 30000
    property int iconRevision: 0
    property string previewError: ""
    property int previewRequests: 0
    property real revealProgress: 0
    readonly property bool compositorClose: PopupTransitions.matches(100, Easing.OutCubic, "bingux-switcher")
    property var history: []
    property var historyOrderIds: []
    property bool historyLoaded: false
    property bool windowsLoaded: false
    property var pendingWindowSnapshot: null
    readonly property string orderStateDirectory: Quickshell.statePath("window-switcher")
    property var liveWindows: []
    property string focusedWindowId: ""
    property var windows: []
    property int selected: 0
    property var activeScreen: Quickshell.screens[0] || null
    readonly property int gridGap: Theme.spaceSmall
    readonly property int cardPadding: Theme.gap
    readonly property int headerHeight: Theme.iconSize + Theme.gap
    // Keep the chooser in the central 40% of the monitor: 30% remains clear
    // on both sides while additional windows wrap onto centered rows.
    readonly property int gridMaxWidth: activeScreen ? Math.max(240, Math.round(activeScreen.width * 0.4) - cardPadding * 2) : 496
    readonly property int previewBaseHeight: 120
    readonly property int gridWidth: cardLayout.width
    readonly property var cardLayout: arrangeCards()
    readonly property var selectedWindow: windows.length ? windows[selected] : null
    readonly property int displayCount: windows.length
    readonly property var visibleWindows: windows
    readonly property var cardWindows: active || revealProgress > 0 ? windows : history
    readonly property int cardCount: cards.count
    readonly property var selectedIcon: {
        const revision = iconRevision;
        return cards.itemAt(selected)?.appIcon ?? null;
    }
    signal opening
    function restoreHistory(text) {
        try {
            const state = JSON.parse(text);
            if (!state || state.version !== 1 || !Array.isArray(state.windows) || state.windows.length > 512 || state.windows.some(id => typeof id !== "string" || id.length === 0 || id.length > 64))
                throw new Error("Invalid saved window order");
            historyOrderIds = [...new Set(state.windows)];
        } catch (error) {
            historyOrderIds = [];
            console.warn("bingux-switcher: unable to restore window order:", error);
        }
        historyLoaded = true;
        if (pendingWindowSnapshot !== null) {
            const snapshot = pendingWindowSnapshot;
            pendingWindowSnapshot = null;
            refresh(snapshot);
        }
    }
    function persistHistory(olderIds) {
        const ids = history.slice(0, 512).map(window => String(window.id));
        for (const id of olderIds)
            if (ids.length < 512 && !ids.includes(id))
                ids.push(id);
        if (JSON.stringify(ids) === JSON.stringify(historyOrderIds))
            return;
        historyOrderIds = ids;
        historyOrderFile.setText(JSON.stringify({version: 1, windows: ids}));
    }
    Process {
        command: ["mkdir", "-p", "-m", "700", root.orderStateDirectory]
        running: true
        onExited: code => {
            if (code === 0)
                historyOrderFile.path = root.orderStateDirectory + "/order.json";
            else {
                console.warn("bingux-switcher: unable to create window-order state directory");
                root.restoreHistory("{\"version\":1,\"windows\":[]}");
            }
        }
    }
    onShownChanged: {
        if (shown)
            shownAt = Date.now();
        visibilityAnimation.stop();
        visibilityAnimation.to = shown ? 1 : 0;
        visibilityAnimation.duration = Theme.reducedMotion || shown || compositorClose ? 0 : 100;
        visibilityAnimation.start();
    }
    NumberAnimation {
        id: visibilityAnimation
        target: root
        property: "revealProgress"
        easing.type: Easing.OutCubic
        onFinished: if (!root.active)
            root.windows = []
    }
    function refresh(snapshot) {
        if (!historyLoaded) {
            pendingWindowSnapshot = snapshot;
            return;
        }
        windowsLoaded = true;
        const live = snapshot;
        const retainedPreviews = {};
        const retainedTimes = {};
        for (const window of live)
            if (previews[window.id])
                retainedPreviews[window.id] = previews[window.id];
        for (const window of live)
            if (previewTimes[window.id])
                retainedTimes[window.id] = previewTimes[window.id];
        previews = retainedPreviews;
        previewTimes = retainedTimes;
        // Resolve icons as windows change, before a shortcut reveals the card.
        for (const window of live)
            OsIcons.resolve(iconFor(window));
        liveWindows = live;
        const liveIds = new Set(live.map(window => String(window.id)));
        const orderMatchesLive = historyOrderIds.some(id => liveIds.has(id));
        const savedIds = live.length === 0 || orderMatchesLive ? historyOrderIds : [];
        const remembered = savedIds.map(id => live.find(window => String(window.id) === id)).filter(Boolean);
        const rememberedIds = new Set(remembered.map(window => String(window.id)));
        const newWindows = live.filter(window => !rememberedIds.has(String(window.id))).sort((a, b) => (b.lastUserTime || 0) - (a.lastUserTime || 0));
        history = remembered.concat(newWindows);
        const focused = live.find(window => window.focused);
        const focusedId = focused ? focused.id : "";
        if (focusedId !== focusedWindowId) {
            focusedWindowId = focusedId;
            warmPreview.restart();
        }
        if (focused)
            history = [focused].concat(history.filter(window => window.id !== focused.id));
        persistHistory(savedIds.filter(id => !liveIds.has(id)));
        if (!active)
            return;
        const previous = selectedWindow;
        windows = windows.map(window => live.find(next => next.id === window.id)).filter(Boolean);
        const retained = previous ? windows.findIndex(window => window.id === previous.id) : -1;
        selected = retained >= 0 ? retained : Math.max(0, Math.min(selected, windows.length - 1));
        if (!windows.length)
            cancel();
    }
    function step(backwards) {
        if (!active) {
            windows = history.slice();
            if (!windows.length) {
                shortcuts.end();
                return;
            }
            active = true;
            warmPreview.stop();
            previewAttempts = ({});
            selected = backwards ? windows.length - 1 : windows.length > 1 ? 1 : 0;
            const focused = history.find(window => window.focused);
            activeScreen = focused && focused.monitor ? Quickshell.screens.find(screen => screen.x === focused.monitor.x && screen.y === focused.monitor.y) || Quickshell.screens[0] : Quickshell.screens[0] || null;
            opening();
            if (root.showDelay === 0)
                root.shown = true;
            else
                reveal.restart();
        } else {
            selected = (selected + (backwards ? -1 : 1) + windows.length) % windows.length;
            previewPump.restart();
        }
    }
    function cancel() {
        reveal.stop();
        active = false;
        shown = false;
        if (revealProgress === 0)
            windows = [];
    }
    function close() {
        cancel();
        shortcuts.end();
    }
    function finish() {
        if (!active)
            return;
        const window = selectedWindow;
        cancel();
        if (window && liveWindows.some(live => live.id === window.id))
            shortcuts.activateWindow(window.id);
    }
    function appFor(window) {
        if (!window || !window.appId)
            return null;
        const id = window.appId.replace(/\.desktop$/, "");
        return DesktopEntries.byId(id) || DesktopEntries.heuristicLookup(id);
    }
    function appName(window) {
        const app = appFor(window);
        return app && app.name ? app.name : window ? window.title || "Window" : "";
    }
    function iconFor(window) {
        const app = appFor(window);
        return Quickshell.iconPath(app && app.icon ? app.icon : "application-x-executable", "application-x-executable");
    }
    function previewAspect(window) {
        const geometry = window?.geometry;
        return geometry && geometry.width > 0 && geometry.height > 0 ? geometry.width / geometry.height : 16 / 9;
    }
    function previewSize(window) {
        const aspect = root.previewAspect(window);
        const width = Math.min(root.gridMaxWidth - Theme.gap * 2, 320, Math.round(root.previewBaseHeight * aspect));
        return {
            width: Math.max(1, width),
            height: Math.max(1, Math.round(width / aspect))
        };
    }
    function cardWidthFor(window) {
        return Math.max(124, root.previewSize(window).width + Theme.gap * 2);
    }
    function cardHeightFor(window) {
        return root.headerHeight + root.previewSize(window).height + Theme.gap * 2 + Theme.spaceSmall;
    }
    function arrangeCards() {
        const positions = [];
        let row = [], rowWidth = 0, rowHeight = 0, top = 0, widestRow = 0;
        function placeRow() {
            widestRow = Math.max(widestRow, rowWidth);
            let left = (root.gridMaxWidth - rowWidth) / 2;
            for (const entry of row) {
                positions[entry.index] = {
                    x: left,
                    y: top
                };
                left += entry.width + root.gridGap;
            }
            top += rowHeight + root.gridGap;
            row = [];
            rowWidth = 0;
            rowHeight = 0;
        }
        root.windows.forEach((window, index) => {
            const width = root.cardWidthFor(window);
            if (row.length && rowWidth + root.gridGap + width > root.gridMaxWidth)
                placeRow();
            rowWidth += (row.length ? root.gridGap : 0) + width;
            rowHeight = Math.max(rowHeight, root.cardHeightFor(window));
            row.push({
                index,
                width
            });
        });
        if (row.length)
            placeRow();
        // Wrap against the monitor limit, then trim the unused space equally
        // from both sides without changing the row breaks or their centering.
        const inset = (root.gridMaxWidth - widestRow) / 2;
        for (const position of positions)
            position.x -= inset;
        return {
            positions,
            width: widestRow,
            height: Math.max(0, top - root.gridGap)
        };
    }
    Timer {
        id: reveal
        interval: root.showDelay
        onTriggered: if (root.active)
            root.shown = true
    }
    function needsPreview(window, now) {
        // Refresh the window being used, but reuse background snapshots across gestures.
        const maximumAge = window.focused ? 2000 : previewCacheAge;
        return !previews[window.id] || now - (previewTimes[window.id] || 0) > maximumAge;
    }
    function requestPreview(window) {
        previewPending = true;
        previewTimeout.restart();
        previewRequests++;
        const size = root.previewSize(window);
        shortcuts.requestPreview(window.id, size.width, size.height);
    }
    // One idle snapshot after focus settles prepares the next switch and keeps
    // content for clients that release their buffers when minimised.
    Timer {
        id: warmPreview
        interval: 250
        onTriggered: {
            if (!root.enabled || root.active || root.previewPending)
                return;
            const focused = root.liveWindows.find(window => window.id === root.focusedWindowId);
            if (focused && root.needsPreview(focused, Date.now()))
                root.requestPreview(focused);
        }
    }
    // Capture each visible window at most once per gesture, after navigation
    // settles. Keep the last thumbnail on screen while a replacement is decoded.
    Timer {
        id: previewPump
        interval: 120
        repeat: true
        running: root.shown && !root.previewPending
        onTriggered: {
            if (!root.shown || root.previewPending)
                return;
            const now = Date.now();
            const targets = [root.selectedWindow].concat(root.visibleWindows);
            const target = targets.find(window => window && !root.previewAttempts[window.id] && root.needsPreview(window, now));
            if (!target) {
                stop();
                return;
            }
            root.previewAttempts = Object.assign({}, root.previewAttempts, {
                [target.id]: true
            });
            root.requestPreview(target);
        }
    }
    Timer {
        id: previewTimeout
        interval: 1500
        onTriggered: root.previewPending = false
    }
    ShortcutSession {
        id: shortcuts
        trackWindows: true
        onWindowSnapshot: function (windows) {
            root.refresh(windows);
        }
        onPreviewReceived: function (id, source, error) {
            root.previewPending = false;
            root.previewError = error;
            previewTimeout.stop();
            if (source && root.liveWindows.some(window => window.id === id)) {
                root.previews = Object.assign({}, root.previews, {
                    [id]: source
                });
                root.previewTimes = Object.assign({}, root.previewTimes, {
                    [id]: Date.now()
                });
            }
        }
        enabled: root.enabled && root.historyLoaded && root.windowsLoaded
        bindings: [
            {
                id: "switcher-forward",
                accelerator: "<Alt>Tab",
                hold: 8
            },
            {
                id: "switcher-backward",
                accelerator: "<Alt><Shift>Tab",
                hold: 8
            },
            {
                id: "switcher-super-forward",
                accelerator: "<Super>Tab",
                hold: 67108864
            },
            {
                id: "switcher-super-backward",
                accelerator: "<Super><Shift>Tab",
                hold: 67108864
            }
        ]
        onActivated: function (id, first, modifiers) {
            if (first)
                shortcuts.send({
                    op: "ui-session",
                    action: "command",
                    name: "search",
                    command: {
                        action: "close"
                    }
                });
            root.step(id.indexOf("backward") >= 0);
        }
        onReleased: root.finish()
        onCancelled: root.cancel()
        onFailed: function (message) {
            console.warn("bingux-switcher: " + message);
        }
        onPointerPressed: function (x, y, button) {
            const point = grid.mapFromItem(window.contentItem, x - root.activeScreen.x, y - root.activeScreen.y);
            let clicked = -1;
            for (let index = 0; index < cards.count; index++) {
                const card = cards.itemAt(index);
                if (card && point.x >= card.x && point.x < card.x + card.width && point.y >= card.y && point.y < card.y + card.height) {
                    clicked = index;
                    break;
                }
            }
            if (root.shown && button === 1 && clicked >= 0) {
                root.selected = clicked;
                root.finish();
                shortcuts.end();
            } else
                root.close();
        }
        onKeyPressed: function (key, modifiers) {
            if (key === 65307) {
                root.cancel();
                shortcuts.end();
            } else if (key === 65293 || key === 65421) {
                root.finish();
                shortcuts.end();
            } else if (key === 65361 || key === 65056)
                root.step(true);
            else if (key === 65363 || key === 65289)
                root.step(key === 65289 && (modifiers & 1) !== 0);
        }
    }
    FileView {
        id: historyOrderFile
        path: ""
        atomicWrites: true
        printErrors: false
        onLoaded: root.restoreHistory(text())
        onLoadFailed: root.restoreHistory("{\"version\":1,\"windows\":[]}")
    }
    FileView {
        path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/switcher.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const settings = JSON.parse(text());
                if (typeof settings !== "object" || settings === null || Array.isArray(settings) || Object.keys(settings).some(key => ["enabled", "showDelay"].indexOf(key) < 0) || (settings.enabled !== undefined && typeof settings.enabled !== "boolean") || (settings.showDelay !== undefined && (!Number.isInteger(settings.showDelay) || settings.showDelay < 0 || settings.showDelay > 500)))
                    throw new Error("Invalid switcher settings");
                root.cancel();
                shortcuts.end();
                root.enabled = settings.enabled === undefined ? true : settings.enabled;
                root.showDelay = settings.showDelay === undefined ? 0 : settings.showDelay;
            } catch (error) {
                console.warn("bingux-switcher: keeping previous settings: " + error);
            }
        }
    }
    IpcHandler {
        target: "switcher"
        function status(): string {
            return JSON.stringify({
                active: root.active,
                shown: root.shown,
                ready: shortcuts.ready,
                shownAt: root.shownAt,
                previewCount: Object.keys(root.previews).length,
                previewRequests: root.previewRequests,
                previewError: root.previewError,
                selected: root.active && root.selectedWindow ? root.selectedWindow.title : null,
                windows: root.windows.map(window => window.title)
            });
        }
        function close(): void {
            root.close();
        }
    }
    PanelWindow {
        id: window
        objectName: "switcherWindow"
        screen: root.activeScreen
        visible: root.shown || root.revealProgress > 0
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "bingux-switcher"
        WlrLayershell.layer: WlrLayer.Overlay
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
        BlurRegion {
            enabled: !standardBlur.available
            window: window
            surfaceNamespace: "bingux-switcher"
            region: Qt.rect(presentation.x - 1, presentation.y - 1, presentation.width + 2, presentation.height + 2)
        }
        Item {
            id: presentation
            SurfaceFade {
                target: presentation
            }
            anchors.centerIn: parent
            width: strip.width
            height: strip.height
            opacity: root.compositorClose ? 1 : root.revealProgress
            scale: 1
            Rectangle {
                id: strip
                BackgroundEffect {
                    id: standardBlur
                    target: strip
                    radius: strip.radius
                }
                anchors.horizontalCenter: parent.horizontalCenter
                width: root.gridWidth + root.cardPadding * 2
                height: grid.implicitHeight + root.cardPadding * 2
                radius: Theme.cardRadius
                color: Theme.overlaySurface
                border.width: 1
                border.color: Theme.outline
                PanelOutline {
                    surface: strip
                }
                PopupShadow {
                    surface: strip
                }
                Item {
                    id: grid
                    objectName: "switcherGrid"
                    anchors.centerIn: parent
                    width: root.gridWidth
                    implicitHeight: root.cardLayout.height
                    Repeater {
                        id: cards
                        // Keep one card per live window ready while unmapped.
                        // A shortcut only updates order and selection.
                        model: root.cardWindows.length
                        onItemAdded: root.iconRevision++
                        onItemRemoved: root.iconRevision++
                        Item {
                            required property int index
                            readonly property var modelData: root.cardWindows[index] || ({
                                    id: "",
                                    appId: ""
                                })
                            readonly property var previewDimensions: root.previewSize(modelData)
                            readonly property bool selectedCard: index === root.selected
                            property alias appIcon: icon
                            objectName: selectedCard ? "switcherSelection" : "switcherCard"
                            width: root.cardWidthFor(modelData)
                            height: root.cardHeightFor(modelData)
                            x: root.cardLayout.positions[index]?.x ?? 0
                            y: root.cardLayout.positions[index]?.y ?? 0
                            ControlCentreButtonSurface {
                                anchors.fill: parent
                                control: QtObject {
                                    readonly property bool enabled: true
                                    readonly property bool hovered: false
                                    readonly property bool down: false
                                    readonly property bool visualFocus: false
                                }
                                radius: Theme.insetRadius(Theme.cardRadius, root.cardPadding)
                                baseColor: Theme.hover
                                opacity: selectedCard ? 1 : 0
                                Behavior on opacity {
                                    enabled: root.shown && !Theme.reducedMotion
                                    NumberAnimation {
                                        duration: 80
                                        easing.type: Easing.OutCubic
                                    }
                                }
                            }
                            ColumnLayout {
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                spacing: Theme.spaceSmall
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: root.headerHeight
                                    spacing: Theme.gap
                                    AppIcon {
                                        id: icon
                                        objectName: "switcherAppIcon"
                                        implicitSize: 24
                                        badgeSize: 12
                                        shadowed: false
                                        Layout.preferredWidth: 24
                                        Layout.preferredHeight: 24
                                        group: ({
                                                id: modelData.appId.replace(/\.desktop$/, ""),
                                                displayName: root.appName(modelData),
                                                desktopEntry: root.appFor(modelData),
                                                windows: [modelData]
                                            })
                                        activeStreams: root.activeStreams
                                        notifications: root.notifications
                                    }
                                    Text {
                                        objectName: "switcherAppTitle"
                                        Layout.fillWidth: true
                                        text: modelData.title || root.appName(modelData)
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontSize
                                    }
                                }
                                ClippingRectangle {
                                    id: previewSurface
                                    objectName: "switcherPreview"
                                    Layout.alignment: Qt.AlignHCenter
                                    Layout.preferredWidth: previewDimensions.width
                                    Layout.preferredHeight: previewDimensions.height
                                    radius: Theme.barControlRadius
                                    color: Theme.background
                                    Image {
                                        id: previewImage
                                        anchors.fill: parent
                                        source: root.previews[modelData.id] || ""
                                        fillMode: Image.PreserveAspectCrop
                                        verticalAlignment: Image.AlignTop
                                        cache: true
                                        asynchronous: true
                                        retainWhileLoading: true
                                    }
                                    OsIconImage {
                                        anchors.centerIn: parent
                                        implicitSize: 36
                                        source: root.iconFor(modelData)
                                        opacity: 0.3
                                        visible: previewImage.status !== Image.Ready
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
