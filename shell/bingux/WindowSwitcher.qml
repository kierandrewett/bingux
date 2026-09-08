import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

Scope {
    id: root
    property bool enabled: true
    property int showDelay: 80
    property bool active: false
    property bool shown: false
    property var activeStreams: []
    property var notifications: []
    property var previews: ({})
    property bool previewPending: false
    property var previewTimes: ({})
    property var previewAttempts: ({})
    property int iconRevision: 0
    property string previewError: ""
    property int previewRequests: 0
    property real revealProgress: 0
    property var history: []
    property var liveWindows: []
    property string focusedWindowId: ""
    property var windows: []
    property int selected: 0
    property int trackIndex: 0
    property int trackStart: 0
    property bool rebasing: false
    property var activeScreen: Quickshell.screens[0] || null
    readonly property int tileWidth: 240
    readonly property int previewHeight: 126
    readonly property int tileHeight: previewHeight + Theme.dockIconSize + Theme.gap * 3
    readonly property int tileGap: Theme.spaceSmall
    readonly property int cardPadding: Theme.dockPadding
    readonly property var selectedWindow: windows.length ? windows[selected] : null
    readonly property int visibleCount: Math.max(1, Math.min(5,
        Math.floor(((activeScreen ? activeScreen.width : 1280) - 64 - cardPadding * 2 + tileGap) / (tileWidth + tileGap))))
    readonly property int displayCount: Math.min(windows.length, visibleCount)
    readonly property var visibleWindows: Array.from({length: displayCount}, (_, index) => windows[(trackStart + index) % windows.length])
    readonly property var selectedIcon: {
        const revision = iconRevision;
        return icons.itemAt(trackIndex)?.appIcon ?? null;
    }
    signal opening()
    onShownChanged: {
        visibilityAnimation.stop();
        visibilityAnimation.to = shown ? 1 : 0;
        visibilityAnimation.duration = Theme.reducedMotion ? 0 : shown ? Theme.popupOpenMotion : Theme.popupCloseMotion;
        visibilityAnimation.start();
    }
    NumberAnimation {
        id: visibilityAnimation
        target: root
        property: "revealProgress"
        easing.type: Easing.OutCubic
        onFinished: if (!root.active) root.windows = []
    }
    function revealSelection() {
        if (trackIndex < trackStart) trackStart = trackIndex;
        else if (trackIndex >= trackStart + displayCount) trackStart = trackIndex - displayCount + 1;
        recenter.restart();
    }
    function normalizeTrack() {
        if (!windows.length || (trackIndex >= windows.length && trackIndex < windows.length * 2)) return;
        const copies = 1 - Math.floor(trackIndex / windows.length);
        const pixels = copies * windows.length * (tileWidth + tileGap);
        const highlightX = selectionHighlight.x + pixels;
        const scrollX = viewport.contentX + pixels;
        rebasing = true;
        trackIndex += copies * windows.length;
        trackStart += copies * windows.length;
        // Preserve the current interpolated positions as well as the targets.
        // This also works while key repeat keeps an animation in progress.
        selectionHighlight.x = highlightX;
        viewport.contentX = scrollX;
        rebasing = false;
        selectionHighlight.x = Qt.binding(() => root.trackIndex * (root.tileWidth + root.tileGap));
        viewport.contentX = Qt.binding(() => root.trackStart * (root.tileWidth + root.tileGap));
    }
    Timer {
        id: recenter
        interval: Theme.reducedMotion ? 0 : Theme.motion + 32
        onTriggered: {
            if (!root.active || !root.windows.length) return;
            root.normalizeTrack();
        }
    }

    function refresh(snapshot) {
        const live = snapshot;
        const retainedPreviews = {};
        const retainedTimes = {};
        for (const window of live) if (previews[window.id]) retainedPreviews[window.id] = previews[window.id];
        for (const window of live) if (previewTimes[window.id]) retainedTimes[window.id] = previewTimes[window.id];
        previews = retainedPreviews;
        previewTimes = retainedTimes;
        // Resolve icons as windows change, before a shortcut reveals the card.
        for (const window of live) OsIcons.resolve(iconFor(window));
        liveWindows = live;
        const previousIds = history.map(window => window.id);
        history = history.map(window => live.find(next => next.id === window.id)).filter(Boolean)
            .concat(live.filter(window => previousIds.indexOf(window.id) < 0).sort((a, b) => (b.lastUserTime || 0) - (a.lastUserTime || 0)));
        const focused = live.find(window => window.focused);
        const focusedId = focused ? focused.id : "";
        if (focusedId !== focusedWindowId) {
            focusedWindowId = focusedId;
            warmPreview.restart();
        }
        if (focused) history = [focused].concat(history.filter(window => window.id !== focused.id));
        if (!active) return;
        const previous = selectedWindow;
        const previousCount = windows.length;
        windows = windows.map(window => live.find(next => next.id === window.id)).filter(Boolean);
        const retained = previous ? windows.findIndex(window => window.id === previous.id) : -1;
        selected = retained >= 0 ? retained : Math.max(0, Math.min(selected, windows.length - 1));
        if (windows.length !== previousCount) {
            rebasing = true;
            trackIndex = windows.length + selected;
            trackStart = windows.length + Math.max(0, selected - displayCount + 1);
            rebasing = false;
        }
        if (!windows.length) cancel();
    }
    function step(backwards) {
        if (!active) {
            windows = history.slice();
            if (!windows.length) { shortcuts.end(); return; }
            active = true;
            warmPreview.stop();
            previewAttempts = ({});
            selected = backwards ? windows.length - 1 : windows.length > 1 ? 1 : 0;
            trackIndex = windows.length + selected;
            trackStart = windows.length + Math.max(0, selected - displayCount + 1);
            const focused = history.find(window => window.focused);
            activeScreen = focused && focused.monitor ? Quickshell.screens.find(screen =>
                screen.x === focused.monitor.x && screen.y === focused.monitor.y) || Quickshell.screens[0]
                : Quickshell.screens[0] || null;
            opening();
            reveal.restart();
        } else {
            normalizeTrack();
            selected = (selected + (backwards ? -1 : 1) + windows.length) % windows.length;
            trackIndex += backwards ? -1 : 1;
            revealSelection();
            previewPump.restart();
        }
    }
    function cancel() {
        reveal.stop();
        active = false;
        shown = false;
        if (revealProgress === 0) windows = [];
    }
    function close() { cancel(); shortcuts.end(); }
    function finish() {
        if (!active) return;
        const window = selectedWindow;
        cancel();
        if (window && liveWindows.some(live => live.id === window.id)) shortcuts.activateWindow(window.id);
    }
    function appFor(window) {
        if (!window || !window.appId) return null;
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
    Timer { id: reveal; interval: root.showDelay; onTriggered: if (root.active) root.shown = true }
    function requestPreview(window) {
        previewPending = true;
        previewTimeout.restart();
        previewRequests++;
        shortcuts.requestPreview(window.id, tileWidth - Theme.gap * 2, previewHeight);
    }
    // One idle snapshot after focus settles prepares the next switch and keeps
    // content for clients that release their buffers when minimised.
    Timer {
        id: warmPreview
        interval: 250
        onTriggered: {
            if (!root.enabled || root.active || root.previewPending) return;
            const focused = root.liveWindows.find(window => window.id === root.focusedWindowId);
            if (focused && (!root.previews[focused.id] || Date.now() - (root.previewTimes[focused.id] || 0) > 2000))
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
            if (!root.shown || root.previewPending) return;
            const now = Date.now();
            const targets = [root.selectedWindow].concat(root.visibleWindows);
            const target = targets.find(window => window && !root.previewAttempts[window.id]
                && (!root.previews[window.id] || now - (root.previewTimes[window.id] || 0) > 2000));
            if (!target) { stop(); return; }
            root.previewAttempts = Object.assign({}, root.previewAttempts, {[target.id]: true});
            root.requestPreview(target);
        }
    }
    Timer { id: previewTimeout; interval: 1500; onTriggered: root.previewPending = false }
    ShortcutSession {
        id: shortcuts
        trackWindows: true
        onWindowSnapshot: function(windows) { root.refresh(windows); }
        onPreviewReceived: function(id, source, error) {
            root.previewPending = false;
            root.previewError = error;
            previewTimeout.stop();
            if (source && root.liveWindows.some(window => window.id === id))
            {
                root.previews = Object.assign({}, root.previews, {[id]: source});
                root.previewTimes = Object.assign({}, root.previewTimes, {[id]: Date.now()});
            }
        }
        enabled: root.enabled
        bindings: [
            {id: "switcher-forward", accelerator: "<Alt>Tab", hold: 8},
            {id: "switcher-backward", accelerator: "<Alt><Shift>Tab", hold: 8},
            {id: "switcher-super-forward", accelerator: "<Super>Tab", hold: 67108864},
            {id: "switcher-super-backward", accelerator: "<Super><Shift>Tab", hold: 67108864}
        ]
        onActivated: function(id, first, modifiers) { root.step(id.indexOf("backward") >= 0); }
        onReleased: root.finish()
        onCancelled: root.cancel()
        onFailed: function(message) { console.warn("bingux-switcher: " + message); }
        onPointerPressed: function(x, y, button) {
            const point = iconRow.mapFromItem(window.contentItem, x - root.activeScreen.x, y - root.activeScreen.y);
            x = point.x;
            y = point.y;
            if (root.shown && button === 1 && x >= 0 && x < iconRow.width && y >= 0 && y < root.tileHeight
                && x % (root.tileWidth + root.tileGap) < root.tileWidth) {
                root.trackIndex = Math.floor(x / (root.tileWidth + root.tileGap));
                root.selected = root.trackIndex % root.windows.length;
                root.finish();
                shortcuts.end();
            } else root.close();
        }
        onKeyPressed: function(key, modifiers) {
            if (key === 65307) { root.cancel(); shortcuts.end(); }
            else if (key === 65293 || key === 65421) { root.finish(); shortcuts.end(); }
            else if (key === 65361 || key === 65056) root.step(true);
            else if (key === 65363 || key === 65289) root.step(key === 65289 && (modifiers & 1) !== 0);
        }
    }
    FileView {
        path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux/switcher.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const settings = JSON.parse(text());
                if (typeof settings !== "object" || settings === null || Array.isArray(settings)
                    || Object.keys(settings).some(key => ["enabled", "showDelay"].indexOf(key) < 0)
                    || (settings.enabled !== undefined && typeof settings.enabled !== "boolean")
                    || (settings.showDelay !== undefined && (!Number.isInteger(settings.showDelay) || settings.showDelay < 0 || settings.showDelay > 500)))
                    throw new Error("Invalid switcher settings");
                root.cancel();
                shortcuts.end();
                root.enabled = settings.enabled === undefined ? true : settings.enabled;
                root.showDelay = settings.showDelay === undefined ? 80 : settings.showDelay;
            } catch (error) { console.warn("bingux-switcher: keeping previous settings: " + error); }
        }
    }
    IpcHandler {
        target: "switcher"
        function status(): string {
            return JSON.stringify({active: root.active, shown: root.shown, ready: shortcuts.ready,
                previewCount: Object.keys(root.previews).length, previewRequests: root.previewRequests, previewError: root.previewError,
                selected: root.active && root.selectedWindow ? root.selectedWindow.title : null,
                windows: root.windows.map(window => window.title)});
        }
        function close(): void { root.close(); }
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
        anchors { top: true; bottom: true; left: true; right: true }
        mask: Region { width: 0; height: 0 }
        Item {
            id: presentation
            anchors.centerIn: parent
            width: Math.max(strip.width, caption.width)
            height: strip.height
            opacity: root.revealProgress
            scale: Theme.popupInitialScale + (1 - Theme.popupInitialScale) * root.revealProgress
            Rectangle {
                id: strip
                anchors.horizontalCenter: parent.horizontalCenter
                width: root.displayCount * (root.tileWidth + root.tileGap) - root.tileGap + root.cardPadding * 2
                height: root.tileHeight + root.cardPadding * 2
                radius: Theme.shellRadius
                color: Theme.shellSurface
                border.width: 1
                border.color: Theme.outline
                Flickable {
                    id: viewport
                    objectName: "switcherViewport"
                    anchors.fill: parent
                    anchors.margins: root.cardPadding
                    contentWidth: iconRow.implicitWidth
                    contentHeight: root.tileHeight
                    contentX: root.trackStart * (root.tileWidth + root.tileGap)
                    interactive: false
                    clip: true
                    Behavior on contentX {
                        enabled: root.shown && root.revealProgress > 0 && !root.rebasing && !Theme.reducedMotion
                        NumberAnimation { duration: Theme.motion; easing.type: Easing.OutCubic }
                    }
                Rectangle {
                    id: selectionHighlight
                    objectName: "switcherSelection"
                    x: root.trackIndex * (root.tileWidth + root.tileGap)
                    y: 0
                    width: root.tileWidth
                    height: root.tileHeight
                    radius: Theme.insetRadius(strip.radius, root.cardPadding)
                    color: Theme.hover
                    Behavior on x {
                        enabled: root.shown && root.revealProgress > 0 && !root.rebasing && !Theme.reducedMotion
                        NumberAnimation { duration: Theme.motion; easing.type: Easing.OutCubic }
                    }
                }
                RowLayout {
                    id: iconRow
                    spacing: root.tileGap
                    Repeater {
                        id: icons
                        // A numeric model preserves delegates on title/focus snapshots.
                        // Replacing a JS-array model destroys every badge and image.
                        model: root.shown || root.revealProgress > 0 ? root.windows.length * 3 : 0
                        onItemAdded: root.iconRevision++
                        onItemRemoved: root.iconRevision++
                        Item {
                            required property int index
                            readonly property var modelData: root.windows[index % root.windows.length] || ({id: "", appId: ""})
                            property alias appIcon: icon
                            Layout.preferredWidth: root.tileWidth
                            Layout.preferredHeight: root.tileHeight
                            AppIcon {
                                id: icon
                                anchors.left: parent.left
                                anchors.leftMargin: Theme.gap
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: Theme.gap
                                implicitSize: Theme.dockIconSize
                                group: ({id: modelData.appId.replace(/\.desktop$/, ""),
                                    displayName: root.appName(modelData), desktopEntry: root.appFor(modelData), windows: [modelData]})
                                activeStreams: root.activeStreams
                                notifications: root.notifications
                            }
                            Rectangle {
                                id: previewSurface
                                anchors.top: parent.top
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.margins: Theme.gap
                                height: root.previewHeight
                                radius: Theme.insetRadius(strip.radius - root.cardPadding, Theme.gap)
                                color: Theme.background
                                Image {
                                    anchors.fill: parent
                                    anchors.margins: Theme.spaceSmall
                                    source: root.previews[modelData.id] || ""
                                    fillMode: Image.PreserveAspectFit
                                    cache: true
                                    asynchronous: true
                                    retainWhileLoading: true
                                }
                                OsIconImage {
                                    anchors.centerIn: parent
                                    implicitSize: Theme.dockIconSize
                                    source: root.iconFor(modelData)
                                    opacity: 0.3
                                    visible: !root.previews[modelData.id]
                                }
                            }
                            ColumnLayout {
                                anchors.left: icon.right
                                anchors.leftMargin: Theme.gap
                                anchors.right: parent.right
                                anchors.rightMargin: Theme.gap
                                anchors.verticalCenter: icon.verticalCenter
                                spacing: Theme.spaceSmall
                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.title || root.appName(modelData)
                                    textFormat: Text.PlainText
                                    elide: Text.ElideMiddle
                                    color: Theme.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.appName(modelData)
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSize
                                }
                            }
                        }
                    }
                }
                }
            }
            TooltipBubble {
                id: caption
                objectName: "switcherTooltip"
                anchors.top: strip.bottom
                anchors.topMargin: Theme.gap
                anchors.horizontalCenter: parent.horizontalCenter
                maximumWidth: Math.min(420, window.width - Theme.paddingLarge * 2)
                wrapText: true
                text: {
                    if (!root.selectedWindow) return "";
                    const app = root.appName(root.selectedWindow);
                    const activity = root.selectedIcon ? root.selectedIcon.tooltipText : app;
                    return activity + (root.selectedWindow.title !== app ? "\n" + root.selectedWindow.title : "");
                }
            }
        }
    }
}
