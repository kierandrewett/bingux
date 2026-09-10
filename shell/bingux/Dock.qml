import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQuick.Effects
import QtCore
import QtQml.Models
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets
import Quickshell.Services.Mpris
import "MediaMatch.js" as MediaMatch
import "DesktopLayout.js" as DesktopLayout

PanelWindow {
    id: root
    signal applicationInteracted()
    signal widgetEditRequested(string widgetId, var control)

    readonly property real popupAnchorTop: (screen ? screen.height : 0) - height - margins.bottom + dockSurface.y
    readonly property alias editSurface: dockSurface
    readonly property alias widgetHost: dockWidgets
    readonly property var preferences: DesktopEditing.desktop
    readonly property int itemSize: (preferences.dockSize || 56) + 16
    readonly property int iconSize: preferences.dockSize || 56
    required property var settings
    property var notifications: []
    readonly property var notificationIndex: MediaMatch.notificationIndex(notifications)
    property var notificationStore: null
    property var activity: DockActivity {}
    property var appGroups: []
    // A layer menu can temporarily clear the compositor's active toplevel.
    property var lastActiveWindow: ToplevelManager.activeToplevel
    property var windowFocusHistory: []
    property var minimisedWindowHistory: []
    property var groupRestoreTargets: ({})
    function rememberFocusedWindow() {
        const window = ToplevelManager.activeToplevel;
        if (window)
            windowFocusHistory = [window].concat(windowFocusHistory.filter(previous => previous && previous !== window));
    }
    Connections {
        target: ToplevelManager
        function onActiveToplevelChanged() {
            root.rememberFocusedWindow();
            if (ToplevelManager.activeToplevel)
                root.lastActiveWindow = ToplevelManager.activeToplevel;
        }
    }
    // Keep live groups stable while the compositor publishes transient
    // toplevels or changes the order of its foreign-toplevel list.
    property var observedGroupOrder: []
    property bool appGroupsInitialised: false
    property bool startupReady: false
    property string startupLayoutKey: ""
    visible: startupReady
    Timer {
        id: startupSettle
        interval: 150
        onTriggered: if (BinguxPreferences.loaded) root.finishStartup()
    }
    Timer {
        id: startupDeadline
        interval: 1500
        onTriggered: root.finishStartup()
    }
    Connections {
        target: BinguxPreferences
        function onLoadedChanged() {
            if (!root.startupReady && BinguxPreferences.loaded) {
                root.refreshAppGroups();
                startupSettle.restart();
            }
        }
    }
    function finishStartup() {
        refreshDebounce.stop();
        root.refreshAppGroupsNow();
        startupSettle.stop();
        startupDeadline.stop();
        // Let the Repeater and RowLayout polish their final sizes before map.
        Qt.callLater(() => { root.startupReady = true; orderCache.restart(); });
    }
    Timer {
        id: orderCache
        interval: 500
        onTriggered: {
            const order = root.appGroups.filter(group => !group.exiting).map(group => group.id);
            if (JSON.stringify(order) !== JSON.stringify(dockState.observedOrder)) {
                dockState.observedOrder = order;
                dockState.sync();
            }
        }
    }
    onAppGroupsChanged: rectangleUpdate.restart()
    onWidthChanged: rectangleUpdate.restart()
    onHeightChanged: rectangleUpdate.restart()
    onVisibleChanged: rectangleUpdate.restart()
    onDragOffsetChanged: rectangleUpdate.restart()
    onSettleOffsetChanged: rectangleUpdate.restart()
    Timer {
        id: rectangleUpdate
        interval: 16
        onTriggered: {
            for (let i = 0; i < dockItems.count; i++) {
                const item = dockItems.itemAt(i);
                if (item) item.publishRectangle();
            }
            root.updateTooltipPosition();
        }
    }
    Instantiator {
        model: ToplevelManager.toplevels
        delegate: Connections {
            required property var modelData
            target: modelData
            // The initial app ID can arrive before this Connections object exists.
            Component.onCompleted: root.associatePendingLaunchToplevel(modelData)
            function onAppIdChanged() {
                root.associatePendingLaunchToplevel(modelData);
                root.refreshAppGroups();
            }
            function onParentChanged() { root.refreshAppGroups() }
            // Titles are read directly by menus/tooltips. Only unidentified
            // windows use title presence to decide whether they form a group.
            function onTitleChanged() { if (!modelData.appId) root.refreshAppGroups() }
            function onScreensChanged() { root.refreshAppGroups() }
            function onMinimizedChanged() {
                const remaining = root.minimisedWindowHistory.filter(window => window && window !== modelData);
                root.minimisedWindowHistory = modelData.minimized ? [modelData].concat(remaining) : remaining;
                root.refreshAppGroups();
            }
        }
    }
    property string draggedId: ""
    property real dragOffset: 0
    property int dropIndex: -1
    readonly property bool dropPinned: dropIndex >= 0 && dropIndex < appGroups.length && isPinned(appGroups[dropIndex])
    readonly property int previewPinnedCount: {
        const source = groupIndex(draggedId);
        return source < 0 || dropIndex < 0 ? pinnedGroupCount
            : pinnedGroupCount + Number(dropPinned) - Number(isPinned(appGroups[source]));
    }
    property bool settlingDrag: false
    property bool committingReorder: false
    property string settlingId: ""
    property int settlingTarget: -1
    property real settleOffset: 0
    property var tooltipOwner: null
    readonly property bool tooltipVisible: dockTooltip.visible
    readonly property real dockTopFromBottom: height - dockSurface.y + margins.bottom

    Timer {
        id: refreshDebounce
        // Coalesce compositor notifications into one frame without making a
        // newly opened window feel delayed.
        interval: 16
        onTriggered: root.refreshAppGroupsNow()
    }

    Timer {
        id: tooltipHideDelay
        interval: 120
        onTriggered: {
            if (root.tooltipOwner === null)
                dockTooltip.shown = false;
        }
    }

    DockTooltip {
        id: dockTooltip
        screen: root.screen
        anchorBottom: root.dockTopFromBottom
    }

    PanelWindow {
        id: launchOverlay
        screen: root.screen
        visible: false
        color: "transparent"
        implicitHeight: root.iconSize * 4 + Theme.padding * 2
        anchors { bottom: true; left: true; right: true }
        margins.left: root.margins.left
        margins.right: root.margins.right
        margins.bottom: root.margins.bottom
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "gnoblin-dock-launch"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        mask: Region {}

        function play(icon) {
            if (Theme.reducedMotion)
                return;
            const position = icon.mapToItem(root.contentItem, 0, 0);
            launchOrigin.x = position.x;
            launchOrigin.y = implicitHeight - root.height + position.y;
            launchOrigin.width = icon.width;
            launchOrigin.height = icon.height;
            const source = icon.source;
            visible = true;
            Qt.callLater(() => dockLaunchEffect.play(launchOrigin, source, false));
        }

        Item { id: launchOrigin }
        SearchLaunchEffect {
            id: dockLaunchEffect
            onFinished: launchOverlay.visible = false
        }
    }

    function tooltipEntered(owner) {
        tooltipHideDelay.stop();
    }

    function showTooltip(owner) {
        if (!owner || owner.menuOpen || root.draggedId.length > 0)
            return;

        root.tooltipOwner = owner;
        dockTooltip.text = owner.tooltipText;
        root.updateTooltipPosition();
        dockTooltip.shown = true;
    }

    function updateTooltipPosition() {
        const owner = root.tooltipOwner;
        if (owner)
            dockTooltip.centreX = root.margins.left + owner.mapToItem(root.contentItem, owner.width / 2, 0).x;
    }

    function leaveTooltip(owner) {
        if (root.tooltipOwner !== owner)
            return;

        root.tooltipOwner = null;
        tooltipHideDelay.restart();
    }

    function dismissTooltip() {
        tooltipHideDelay.stop();
        root.tooltipOwner = null;
        dockTooltip.shown = false;
    }

    function closeMenus() {
        dismissTooltip();
        for (let index = 0; index < dockItems.count; ++index) {
            const item = dockItems.itemAt(index);
            if (item) item.menuOpen = false;
        }
    }

    function snapshotLayout() {
        return {pinnedApps: pinnedApps.slice(), order: appOrder.slice(),
            applications: appGroups.filter(group => isPinned(group)).map(group => ({
                id: group.id, desktopId: group.desktopEntry ? group.desktopEntry.id : group.id
            }))};
    }

    Settings {
        id: dockState
        location: "file://" + (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/gnoblin/dock.ini"
        category: "dock"
        property var order: []
        property var pinnedApps: []
        property var unpinnedApps: []
        property var observedOrder: []
    }
    function pinIdentity(appId) {
        const entry = desktopEntryFor(normaliseAppId(appId));
        return normaliseAppId(entry ? entry.id : appId);
    }
    readonly property var pinnedApps: {
        if (preferences.dockApps) return preferences.dockApps.pinnedApps;
        const removed = dockState.unpinnedApps.map(id => pinIdentity(id));
        return [...new Set(settings.pinnedApps.concat(dockState.pinnedApps).map(id => pinIdentity(id)))]
            .filter(id => id.length > 0 && removed.indexOf(id) < 0);
    }
    readonly property var appOrder: preferences.dockApps ? preferences.dockApps.order : dockState.order
    onAppOrderChanged: if (appGroupsInitialised) refreshAppGroups()
    onPinnedAppsChanged: if (appGroupsInitialised) refreshAppGroups()
    function isPinned(group) {
        return pinnedApps.indexOf(pinIdentity(group.desktopEntry ? group.desktopEntry.id : group.id)) >= 0;
    }
    readonly property int pinnedGroupCount: appGroups.filter(group => isPinned(group)).length
    function sectionDestination(id, destination) {
        const source = groupIndex(id);
        if (source < 0) return -1;
        const pinned = isPinned(appGroups[source]);
        const first = pinned ? 0 : pinnedGroupCount;
        const last = pinned ? pinnedGroupCount - 1 : appGroups.length - 1;
        return Math.max(first, Math.min(last, destination));
    }
    function updatePinPreference(group, pinned) {
        if (pinned && !group.desktopEntry) return;
        const id = pinIdentity(group.desktopEntry ? group.desktopEntry.id : group.id);
        if (!id) return;
        if (preferences.dockApps) {
            const pins = pinnedApps.filter(value => pinIdentity(value) !== id);
            if (pinned) pins.push(id);
            BinguxPreferences.saveDesktop({dockApps: {pinnedApps: pins, order: appOrder}});
            return;
        }
        dockState.pinnedApps = dockState.pinnedApps.filter(value => pinIdentity(value) !== id);
        dockState.unpinnedApps = dockState.unpinnedApps.filter(value => pinIdentity(value) !== id);
        if (pinned) dockState.pinnedApps = dockState.pinnedApps.concat([id]);
        else dockState.unpinnedApps = dockState.unpinnedApps.concat([id]);
    }
    function setPinned(group, pinned) {
        updatePinPreference(group, pinned);
        dockState.sync();
        refreshAppGroups();
    }

    function moveGroup(id, destination, pinned) {
        const ids = appGroups.map(group => group.id);
        const source = ids.indexOf(id);
        if (source < 0 || destination < 0 || destination >= ids.length) return;
        if (typeof pinned === "boolean") {
            if (pinned !== isPinned(appGroups[source]))
                updatePinPreference(appGroups[source], pinned);
        } else {
            destination = sectionDestination(id, destination);
            if (source === destination) return;
        }
        ids.splice(source, 1); ids.splice(destination, 0, id);
        const order = ids.concat(appOrder.filter(old => ids.indexOf(old) < 0));
        if (preferences.dockApps) BinguxPreferences.saveDesktop({dockApps: {pinnedApps, order}});
        else dockState.order = order;
        dockState.sync();
        refreshDebounce.stop();
        refreshAppGroupsNow();
    }

    function groupIndex(id) {
        for (let index = 0; index < root.appGroups.length; index++) {
            if (root.appGroups[index].id === id)
                return index;
        }
        return -1;
    }

    function slotPosition(index, pinnedCount) {
        const gap = pinnedCount > 0 && pinnedCount < appGroups.length && index >= pinnedCount ? Theme.padding : 0;
        return index * (root.itemSize + Theme.spaceSmall) + gap;
    }

    function dragDestination(id, offset) {
        const source = groupIndex(id);
        if (source < 0) return -1;
        const position = slotPosition(source, pinnedGroupCount) + offset;
        let destination = source, distance = Infinity;
        for (let index = 0; index < appGroups.length; index++) {
            const candidate = Math.abs(position - slotPosition(index, pinnedGroupCount));
            if (candidate < distance) { distance = candidate; destination = index; }
        }
        // An app without a launcher cannot become a persistent pin.
        return appGroups[source].desktopEntry ? destination : sectionDestination(id, destination);
    }

    function liveReorderShift(index, id) {
        const movingIndex = root.groupIndex(root.draggedId);
        if (movingIndex < 0 || root.dropIndex < 0 || id === root.draggedId)
            return 0;

        let destination = index;
        if (movingIndex < root.dropIndex && index > movingIndex && index <= root.dropIndex)
            destination--;
        if (movingIndex > root.dropIndex && index >= root.dropIndex && index < movingIndex)
            destination++;
        return slotPosition(destination, previewPinnedCount) - slotPosition(index, pinnedGroupCount);
    }
    property var emptyAppIdGroupAssociations: []
    property var launchAttempts: ({})
    property var launchFailures: ({})
    property int launchSequence: 0

    function clearLaunchFailure(id) {
        const failures = Object.assign({}, launchFailures);
        delete failures[id];
        launchFailures = failures;
        launchError.resolve(id);
    }

    function failLaunch(id, serial, message) {
        const attempt = launchAttempts[id];
        if (!attempt || attempt.serial !== serial)
            return;
        const group = appGroups.find(item => item.id === id);
        if (group && group.windows.some(window => !attempt.windows.includes(window)))
            return;
        launchFailures = Object.assign({}, launchFailures, { [id]: {message: message, windows: attempt.windows} });
        const attempts = Object.assign({}, launchAttempts);
        delete attempts[id];
        launchAttempts = attempts;
        if (pendingLaunchGroupId === id) pendingLaunchGroupId = "";
        launchError.show(id, group?.desktopEntry?.name || attempt.name || id, message);
    }

    function animateLaunchFor(groupId) {
        for (let index = 0; index < dockItems.count; index++) {
            const button = dockItems.itemAt(index);
            if (button && button.currentGroup.id === groupId) {
                button.animateLaunch();
                return;
            }
        }
    }

    // External launchers (search, notifications and calendar) dispatch the
    // application themselves. Share the same dock loading state so every
    // launch source presents the same pill while its window is appearing.
    function beginExternalLaunch(appId, name) {
        const entry = desktopEntryFor(normaliseAppId(appId));
        if (!entry)
            return false;
        const group = appGroups.find(item => item.desktopEntry && item.desktopEntry.id === entry.id);
        const groupId = group ? group.id : normaliseAppId(entry.startupClass || entry.id);
        const windows = group ? (group.windows || []).slice() : [];
        clearLaunchFailure(groupId);
        const serial = ++root.launchSequence;
        root.launchAttempts = Object.assign({}, root.launchAttempts, { [groupId]: {
            serial: serial, name: name || entry.name || groupId, windows: windows,
            deadline: Date.now() + Theme.launchTimeout, reportFailure: false
        }});
        animateLaunchFor(groupId);
        return true;
    }

    function endExternalLaunch(appId) {
        const entry = desktopEntryFor(normaliseAppId(appId));
        if (!entry)
            return;
        const group = appGroups.find(item => item.desktopEntry && item.desktopEntry.id === entry.id);
        const groupId = group ? group.id : normaliseAppId(entry.startupClass || entry.id);
        const attempt = launchAttempts[groupId];
        if (!attempt || attempt.reportFailure !== false)
            return;
        const attempts = Object.assign({}, launchAttempts);
        delete attempts[groupId];
        launchAttempts = attempts;
    }

    LaunchErrorDialog {
        id: launchError
        screen: root.screen
        onRetryRequested: id => {
            const group = root.appGroups.find(item => item.id === id);
            if (group) root.launch(group, true);
        }
    }

    Timer {
        interval: 250
        running: Object.keys(root.launchAttempts).length > 0 || Object.keys(root.launchFailures).length > 0
        repeat: true
        onTriggered: {
            const attempts = Object.assign({}, root.launchAttempts);
            for (const id of Object.keys(attempts)) {
                const attempt = attempts[id];
                const group = root.appGroups.find(item => item.id === id);
                if (group && group.windows.some(window => !attempt.windows.includes(window))) {
                    delete attempts[id];
                    root.clearLaunchFailure(id);
                }
            }
            root.launchAttempts = attempts;
            for (const id of Object.keys(attempts)) {
                if (Date.now() < attempts[id].deadline)
                    continue;
                if (attempts[id].reportFailure === false) {
                    const pending = Object.assign({}, root.launchAttempts);
                    delete pending[id];
                    root.launchAttempts = pending;
                } else {
                    root.failLaunch(id, attempts[id].serial, "No new application window appeared. The app may still be starting or running in the background.");
                }
            }
            for (const id of Object.keys(root.launchFailures)) {
                const group = root.appGroups.find(item => item.id === id);
                if (group && group.windows.some(window => !root.launchFailures[id].windows.includes(window))) root.clearLaunchFailure(id);
            }
        }
    }

    property string pendingLaunchGroupId: ""
    property string launchFeedbackToken: ""
    onPendingLaunchGroupIdChanged: {
        if (pendingLaunchGroupId === "") {
            LaunchFeedback.end(launchFeedbackToken);
            launchFeedbackToken = "";
        }
    }
    Component.onDestruction: LaunchFeedback.end(launchFeedbackToken)
    property var pendingLaunchToplevel: null

    function normaliseAppId(appId) {
        if (!appId || appId.length === 0)
            return "";

        return appId.endsWith(".desktop") ? appId.slice(0, -8) : appId;
    }

    property var desktopEntryCache: new Map()
    function desktopEntryFor(appId) {
        if (!appId) return null;
        const cache = desktopEntryCache;
        if (cache.has(appId)) return cache.get(appId);
        const entry = DesktopEntries.byId(appId) || DesktopEntries.byId(appId + ".desktop")
            || DesktopEntries.heuristicLookup(appId) || null;
        cache.set(appId, entry);
        return entry;
    }
    function menuLabel(value, fallback) {
        const text = typeof value === "string" && value.length > 0 ? value : fallback;
        return text.slice(0, 256);
    }

    function associatedGroupIdFor(toplevel) {
        const associations = root.emptyAppIdGroupAssociations;
        for (let index = 0; index < associations.length; index++) {
            const association = associations[index];
            if (association.toplevel === toplevel)
                return association.groupId;

        }
        return "";
    }

    function toplevelGroupId(toplevel) {
        const appId = toplevel && typeof toplevel.appId === "string" ? toplevel.appId : "";
        const normalisedAppId = root.normaliseAppId(appId);

        const desktopEntry = root.desktopEntryFor(normalisedAppId);
        const desktopEntryIdentity = desktopEntry ? root.normaliseAppId(desktopEntry.startupClass || desktopEntry.id) : "";
        return desktopEntryIdentity.length > 0 ? desktopEntryIdentity : normalisedAppId;
    }

    function associatePendingLaunchToplevel(toplevel) {
        const toplevelGroupId = root.toplevelGroupId(toplevel);
        if (!toplevel || root.pendingLaunchGroupId.length === 0)
            return;

        if (toplevelGroupId.length > 0) {
            if (toplevelGroupId !== root.pendingLaunchGroupId)
                return;

            if (root.pendingLaunchToplevel) {
                root.removeToplevelAssociation(root.pendingLaunchToplevel);
                root.pendingLaunchToplevel = null;
            }
            root.pendingLaunchGroupId = "";
            pendingLaunchTimer.stop();
            return;
        }

        if (root.pendingLaunchToplevel || root.associatedGroupIdFor(toplevel).length > 0)
            return;

        const groupId = root.pendingLaunchGroupId;
        const associations = root.emptyAppIdGroupAssociations.slice();
        associations.push({
            "toplevel": toplevel,
            "groupId": groupId
        });
        root.emptyAppIdGroupAssociations = associations;
        root.pendingLaunchToplevel = toplevel;
    }

    function removeToplevelAssociation(toplevel) {
        const associations = [];
        const currentAssociations = root.emptyAppIdGroupAssociations;
        for (let index = 0; index < currentAssociations.length; index++) {
            if (currentAssociations[index].toplevel !== toplevel)
                associations.push(currentAssociations[index]);

        }
        root.emptyAppIdGroupAssociations = associations;
    }

    function refreshAppGroups() {
        // start() leaves an already pending frame in place. A busy window
        // must not postpone all dock updates by repeatedly restarting it.
        refreshDebounce.start();
    }

    function refreshAppGroupsNow() {
        const previousGroups = root.appGroups;
        const previousIds = {};
        const previousGroupsById = {};
        for (const previousGroup of previousGroups) {
            previousIds[previousGroup.id] = true;
            previousGroupsById[previousGroup.id] = previousGroup;
        }
        const observedOrder = root.observedGroupOrder.length > 0
            ? root.observedGroupOrder.slice()
            : previousGroups.map(group => group.id);

        const groups = [];
        const groupIndexes = {
        };
        const discoveryIndexes = {
        };
        const addGroup = function addGroup(appId, fallbackId) {
            const normalisedAppId = root.normaliseAppId(appId);
            if (normalisedAppId.length === 0 && fallbackId.length === 0)
                return -1;

            const desktopEntry = normalisedAppId.length > 0 ? root.desktopEntryFor(normalisedAppId) : null;
            const desktopEntryIdentity = desktopEntry ? root.normaliseAppId(desktopEntry.startupClass || desktopEntry.id) : "";
            const groupId = desktopEntryIdentity.length > 0 ? desktopEntryIdentity : normalisedAppId.length > 0 ? normalisedAppId : fallbackId;
            if (groupIndexes[groupId] !== undefined) {
                const existingGroup = groups[groupIndexes[groupId]];
                if (!existingGroup.desktopEntry && desktopEntry)
                    existingGroup.desktopEntry = desktopEntry;
                return groupIndexes[groupId];
            }

            const group = {
                "id": groupId,
                "desktopEntry": desktopEntry,
                "windows": [],
                "entering": false,
                "exiting": false
            };
            groupIndexes[groupId] = groups.length;
            discoveryIndexes[groupId] = groups.length;
            groups.push(group);
            return groupIndexes[groupId];
        };
        const toplevels = ToplevelManager.toplevels.values;
        for (let index = 0; index < root.pinnedApps.length; index++) {
            addGroup(root.pinnedApps[index], "");
        }
        for (let index = 0; index < toplevels.length; index++) {
            const toplevel = toplevels[index];
            if (!toplevel)
                continue;

            const hasAppId = typeof toplevel.appId === "string" && toplevel.appId.length > 0;
            const associatedGroupId = root.associatedGroupIdFor(toplevel);
            const title = typeof toplevel.title === "string" ? toplevel.title.trim() : "";
            // Gnoblin does not emit optional foreign-toplevel output events.
            // An empty screens list says nothing about window visibility.
            if (toplevel.parent || (!hasAppId && title.length === 0 && associatedGroupId.length === 0))
                continue;

            const fallbackId = !hasAppId && associatedGroupId.length > 0 ? associatedGroupId : "toplevel-" + index;
            const groupIndex = addGroup(toplevel.appId, fallbackId);
            if (groupIndex >= 0)
                groups[groupIndex].windows.push(toplevel);
        }
        const order = appOrder;
        const orderIdentities = order.map(id => pinIdentity(id));
        const ranks = new Map();
        for (const group of groups) {
            const identity = pinIdentity(group.desktopEntry ? group.desktopEntry.id : group.id);
            const index = order.findIndex((id, i) => id === group.id || orderIdentities[i] === identity);
            ranks.set(group, {pinned: pinnedApps.indexOf(identity) >= 0,
                order: index < 0 ? order.length : index});
        }
        groups.sort((a, b) => {
            const aRank = ranks.get(a), bRank = ranks.get(b);
            const sectionDifference = Number(bRank.pinned) - Number(aRank.pinned);
            if (sectionDifference !== 0)
                return sectionDifference;
            const configuredDifference = aRank.order - bRank.order;
            if (configuredDifference !== 0)
                return configuredDifference;

            const observedAi = observedOrder.indexOf(a.id), observedBi = observedOrder.indexOf(b.id);
            const observedDifference = (observedAi < 0 ? observedOrder.length : observedAi) - (observedBi < 0 ? observedOrder.length : observedBi);
            return observedDifference !== 0 ? observedDifference : discoveryIndexes[a.id] - discoveryIndexes[b.id];
        });

        for (const group of groups) {
            const previousGroup = previousGroupsById[group.id];
            if (previousGroup) {
                const previousWindows = previousGroup.windows;
                const discoveredWindows = group.windows.slice();
                group.windows.sort((a, b) => {
                    const ai = previousWindows.indexOf(a), bi = previousWindows.indexOf(b);
                    return (ai < 0 ? previousWindows.length : ai) - (bi < 0 ? previousWindows.length : bi)
                        || discoveredWindows.indexOf(a) - discoveredWindows.indexOf(b);
                });
            }
            group.entering = root.startupReady && !previousIds[group.id];
        }

        const nextObservedOrder = observedOrder.slice();
        for (const group of groups) {
            if (nextObservedOrder.indexOf(group.id) < 0)
                nextObservedOrder.push(group.id);
        }
        root.observedGroupOrder = nextObservedOrder;
        // Keep a departing group's slot until its animation finishes. This
        // is the same ordered model used for layout and drag indices.
        for (let index = 0; index < previousGroups.length; index++) {
            if (!root.startupReady) break;
            const previous = previousGroups[index];
            if (groupIndexes[previous.id] !== undefined)
                continue;
            groups.splice(Math.min(index, groups.length), 0, {
                id: previous.id,
                desktopEntry: previous.desktopEntry,
                windows: [],
                entering: false,
                exiting: true,
            });
        }
        // Retiring slots also belong to their section, including an idle app
        // that has just been unpinned and is animating out.
        // QML's sort can shuffle equal entries. Partition without sorting
        // again so the user's saved order survives within each section.
        root.appGroups = groups.filter(group => root.isPinned(group))
            .concat(groups.filter(group => !root.isPinned(group)));
        root.appGroupsInitialised = true;
        const layoutKey = JSON.stringify([groups.map(group => group.id), root.iconSize,
            root.pinnedGroupCount, root.preferences.layout?.dock]);
        if (!root.startupReady && layoutKey !== root.startupLayoutKey) {
            root.startupLayoutKey = layoutKey;
            startupSettle.restart();
        }
        if (root.startupReady && JSON.stringify(previousGroups.map(group => group.id)) !== JSON.stringify(groups.map(group => group.id)))
            orderCache.restart();
    }

    function finishGroupExit(id) {
        // A reopened app can reverse its departure before this callback.
        if (!root.appGroups.some(group => group.id === id && group.exiting))
            return;
        if (root.draggedId.length > 0)
            root.cancelDrag();
        root.appGroups = root.appGroups.filter(group => group.id !== id || !group.exiting);
    }

    Component {
        id: applicationLaunchProcess
        Process {
            property string groupId: ""
            property int serial: 0
            property string diagnostic: ""
            stdout: SplitParser {
                onRead: data => {
                    if (!data.startsWith("BINGUX_LAUNCH_ERROR ")) return;
                    try {
                        const error = JSON.parse(data.slice(20));
                        root.failLaunch(groupId, serial, error.message);
                    } catch (error) { console.warn("Invalid launch result:", error); }
                }
            }
            stderr: SplitParser { onRead: data => { diagnostic = (diagnostic + "\n" + data).slice(-2000); } }
            onExited: (code, status) => {
                if (code !== 0) root.failLaunch(groupId, serial, diagnostic.trim() || "The application launch service failed.");
                if (code !== 0 && root.pendingLaunchGroupId === groupId) {
                    pendingLaunchTimer.stop();
                    root.pendingLaunchGroupId = "";
                }
                destroy();
            }
        }
    }

    function launch(group, newWindow) {
        const serial = ++root.launchSequence;
        root.clearLaunchFailure(group.id);
        root.launchAttempts = Object.assign({}, root.launchAttempts, { [group.id]: {
            serial: serial, name: group.desktopEntry?.name || group.id, windows: (group.windows || []).slice(), deadline: Date.now() + Theme.dockLaunchTimeout
        }});
        if (!group.desktopEntry) {
            root.failLaunch(group.id, serial, "The installed application could not be found. It may have been removed.");
            return;
        }

        if (root.pendingLaunchToplevel) {
            root.removeToplevelAssociation(root.pendingLaunchToplevel);
            root.pendingLaunchToplevel = null;
        }
        root.pendingLaunchGroupId = group.id;
        LaunchFeedback.end(root.launchFeedbackToken);
        root.launchFeedbackToken = LaunchFeedback.begin(group.id, () => {
            const helper = Quickshell.env("BINGUX_APP_LAUNCHER_HELPER");
            const command = helper ? [helper] : ["python3", decodeURIComponent(Qt.resolvedUrl("launch-application.py").toString().replace(/^file:\/\//, ""))];
            command.push("--dock-feedback");
            if (newWindow) command.push("--new-window");
            command.push("--", group.desktopEntry.id);
            const process = applicationLaunchProcess.createObject(root, {command: command, groupId: group.id, serial: serial});
            process.running = true;
        });
        pendingLaunchTimer.restart();
        root.dismissTooltip();
        animateLaunchFor(group.id);
    }

    function activeWindow(group) {
        // Prefer the compositor's focused identity over per-window state,
        // which can arrive in separate protocol updates.
        const focused = ToplevelManager.activeToplevel;
        if (focused)
            return !focused.minimized && group.windows.indexOf(focused) >= 0 ? focused : null;
        const activated = group.windows.find(window => window && window.activated && !window.minimized);
        if (activated)
            return activated;
        // Clicking a shell surface may briefly clear keyboard focus. Keep
        // that click a minimise action, rather than restoring another window.
        const previous = root.lastActiveWindow;
        return previous && !previous.minimized && group.windows.indexOf(previous) >= 0 ? previous : null;
    }

    function preferredWindow(group) {
        return root.activeWindow(group)
            || root.windowFocusHistory.find(window => group.windows.indexOf(window) >= 0)
            || group.windows[0];
    }

    function toggleGroup(group) {
        if (group.windows.length === 0) {
            root.launch(group);
            return ;
        }
        const active = root.activeWindow(group);
        if (active) {
            root.groupRestoreTargets = Object.assign({}, root.groupRestoreTargets, { [group.id]: active });
            // Hide the other windows first so minimising the focused window
            // cannot expose another window from this app.
            for (const window of group.windows) {
                if (window && window !== active)
                    window.minimized = true;
            }
            active.minimized = true;
            return;
        }
        const remembered = root.groupRestoreTargets[group.id];
        const window = (remembered && group.windows.indexOf(remembered) >= 0 ? remembered : null)
            || root.minimisedWindowHistory.find(candidate => candidate.minimized && group.windows.indexOf(candidate) >= 0)
            || group.windows.find(candidate => candidate && candidate.minimized)
            || root.preferredWindow(group);
        const remainingTargets = Object.assign({}, root.groupRestoreTargets);
        delete remainingTargets[group.id];
        root.groupRestoreTargets = remainingTargets;
        window.minimized = false;
        window.activate();
    }

    function cycleGroup(group, delta) {
        // Scrolling cycles windows that already exist. It must never turn a
        // wheel gesture into an application launch for an idle dock item.
        if (!group || !group.windows || group.windows.length === 0)
            return ;
        const activeIndex = group.windows.indexOf(root.preferredWindow(group));
        const startIndex = activeIndex >= 0 ? activeIndex : 0;
        const direction = delta > 0 ? -1 : 1;
        const nextIndex = (startIndex + direction + group.windows.length) % group.windows.length;
        if (group.windows[nextIndex]) {
            group.windows[nextIndex].minimized = false;
            group.windows[nextIndex].activate();
        }
    }

    exclusiveZone: implicitHeight
    implicitHeight: root.itemSize + 16 + Theme.dockPadding * 2
    mask: Region { item: root.draggedId.length > 0 ? dragCapture : dockInputArea }
    color: "transparent"
    WlrLayershell.layer: DesktopEditing.active ? WlrLayer.Overlay : WlrLayer.Top
    margins.bottom: DesktopEditing.active ? 64 : 0
    Behavior on margins.bottom { NumberAnimation { duration: Theme.reducedMotion ? 0 : 220; easing.type: Easing.OutCubic } }
    WlrLayershell.namespace: "bingux-dock"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    Component.onCompleted: {
        root.observedGroupOrder = dockState.observedOrder.slice();
        startupDeadline.start();
        root.rememberFocusedWindow();
        root.refreshAppGroups();
    }

    Item {
        id: dragCapture
        anchors.fill: parent
        visible: root.draggedId.length > 0
        z: 100

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            onReleased: root.finishDrag()
            onCanceled: root.cancelDrag()
        }
    }

    function finishDrag() {
        if (root.settlingDrag)
            return;

        const id = root.draggedId;
        const target = root.dropIndex;
        const source = root.groupIndex(id);
        if (id.length === 0 || source < 0 || target < 0 || target >= root.appGroups.length) {
            root.cancelDrag();
            return;
        }

        root.settlingId = id;
        root.settlingTarget = target;
        root.settleOffset = root.dragOffset;
        root.settlingDrag = true;
        settleAnimation.from = root.dragOffset;
        settleAnimation.to = slotPosition(target, previewPinnedCount) - slotPosition(source, pinnedGroupCount);
        settleAnimation.restart();
    }

    function completeDrag() {
        if (!root.settlingDrag)
            return;

        const id = root.settlingId;
        const target = root.settlingTarget;
        root.committingReorder = true;
        if (id.length > 0 && target >= 0 && target < root.appGroups.length)
            root.moveGroup(id, target, root.dropPinned);

        // Keep the released icon at the destination while the model reorder
        // is applied, then remove the temporary drag transform in one frame.
        root.dragOffset = 0;
        root.settlingDrag = false;
        root.settlingId = "";
        root.settlingTarget = -1;
        root.settleOffset = 0;
        root.draggedId = "";
        root.dropIndex = -1;
        Qt.callLater(() => { root.committingReorder = false; });
    }

    function cancelDrag() {
        settleAnimation.stop();
        root.settlingDrag = false;
        root.settlingId = "";
        root.settlingTarget = -1;
        root.settleOffset = 0;
        root.draggedId = "";
        root.dragOffset = 0;
        root.dropIndex = -1;
    }

    NumberAnimation {
        id: settleAnimation
        target: root
        property: "settleOffset"
        duration: 180
        easing.type: Easing.OutCubic
        onFinished: root.completeDrag()
    }

    Timer {
        id: pendingLaunchTimer

        interval: Theme.dockLaunchTimeout
        repeat: false
        onTriggered: {
            if (root.pendingLaunchToplevel) {
                root.removeToplevelAssociation(root.pendingLaunchToplevel);
                root.pendingLaunchToplevel = null;
            }
            root.pendingLaunchGroupId = "";
            root.refreshAppGroups();
        }
    }

    anchors {
        bottom: true
        left: true
        right: true
    }

    Item {
        id: dockInputArea
        x: dockSurface.x
        y: dockSurface.y
        width: dockSurface.visible ? dockSurface.width : 0
        height: root.height - y
    }

    Rectangle {
        id: dockSurface
        NativeEditSurface {
            anchors.fill: parent; window: root; zoneName: "dock"
            entries: {
                const result = [];
                for (let i = 0; i < dockItems.count; i++) {
                    const item = dockItems.itemAt(i);
                    if (item) result.push({id: "app:" + root.pinIdentity(item.currentGroup.desktopEntry?.id || item.currentGroup.id), item});
                }
                return result.concat(Object.keys(DesktopEditing.sources).filter(id => root.preferences.layout?.dock.includes(id)).map(id => ({id, item: DesktopEditing.sources[id]})));
            }
        }
        readonly property real itemPadding: Math.max(0, (height - root.itemSize) / 2)
        readonly property real bottomGap: Math.max(0, root.height - y - height)
        onXChanged: rectangleUpdate.restart()
        onYChanged: rectangleUpdate.restart()

        anchors.verticalCenter: parent.verticalCenter
        x: root.preferences.dockAlignment === "left" ? Theme.padding : root.preferences.dockAlignment === "right" ? root.width - width - Theme.padding : (root.width - width) / 2
        width: Math.min(root.width - Theme.padding * 2, Math.max(DesktopEditing.active ? root.itemSize + 16 : 0, dockRow.implicitWidth + dockSurface.itemPadding * 2))
        height: root.itemSize + 16
        radius: Theme.shellRadius
        color: Theme.shellSurface
        border.width: 1
        border.color: Theme.outline
        PanelOutline { surface: dockSurface }
        visible: DesktopEditing.active || root.appGroups.length > 0 || (root.preferences.layout?.dock.length || 0) > 0

        Flickable {
            // Dock editing is handled by NativeEditSurface. Keep the live app
            // buttons inert while the editor is open.
            objectName: "dockLiveWidgets"
            enabled: !DesktopEditing.active
            opacity: DesktopEditing.active ? 0.68 : 1
            anchors.fill: parent
            anchors.leftMargin: dockSurface.itemPadding
            anchors.rightMargin: dockSurface.itemPadding
            // Keep bottom-edge input inside the clipped viewport.
            anchors.bottomMargin: -dockSurface.bottomGap
            onContentXChanged: rectangleUpdate.restart()
            contentWidth: dockRow.implicitWidth
            contentHeight: height
            clip: true
            interactive: root.draggedId.length === 0 && contentWidth > width
            boundsBehavior: Flickable.StopAtBounds
            Rectangle {
                objectName: "dockSectionDivider"
                // Use the section's extent. A cached delegate at the boundary
                // becomes the wrong anchor when ScriptModel moves that icon.
                x: (root.draggedId ? root.previewPinnedCount * root.itemSize : dockRow.sectionWidths.width)
                    + Math.max(0, root.previewPinnedCount - 1) * dockRow.spacing
                    + (dockRow.sectionGap + dockRow.spacing) / 2 - width / 2
                y: (dockSurface.height - height) / 2
                width: 1
                height: root.itemSize / 2
                radius: width / 2
                color: Theme.outline
                opacity: root.draggedId ? (root.previewPinnedCount > 0 && root.previewPinnedCount < root.appGroups.length ? 1 : 0) : dockRow.sectionPresence
                visible: opacity > 0
            }
        RowLayout {
            id: dockRow
            height: dockSurface.height
            spacing: Theme.spaceSmall
            readonly property size sectionWidths: {
                let pinned = 0, running = 0;
                for (let index = 0; index < dockItems.count; index++) {
                    const item = dockItems.itemAt(index);
                    if (!item) continue;
                    if (index < root.pinnedGroupCount) pinned += item.transitionProgress;
                    else running += item.transitionProgress;
                }
                return Qt.size(pinned * root.itemSize, running * root.itemSize);
            }
            readonly property real sectionPresence: Math.min(1, sectionWidths.width / root.itemSize, sectionWidths.height / root.itemSize)
            readonly property real sectionGap: Theme.padding * sectionPresence

            Repeater {
                id: dockItems
                model: ScriptModel {
                    // Keep metadata updates out of reorder operations so each
                    // app retains its button and animation state while moving.
                    values: root.appGroups.map(group => ({ id: group.id }))
                    objectProp: "id"
                }

                delegate: Item {
                    id: dockButton
                    WidgetEditHandle {
                        control: dockButton
                        widgetId: "app:" + root.pinIdentity(dockButton.currentGroup.desktopEntry?.id || dockButton.currentGroup.id)
                        onRequested: (id, item) => root.widgetEditRequested(id, item)
                    }

                    required property var modelData
                    readonly property var currentGroup: root.appGroups.find(group => group.id === modelData.id)
                        ?? { id: modelData.id, desktopEntry: null, windows: [] }
                    required property int index
                    property alias menuOpen: appMenu.visible
                    property bool entering: false
                    readonly property bool exiting: currentGroup.exiting === true
                    property bool presenceReady: false
                    property real transitionProgress: 1
                    property int slideDirection: 1
                    // At zero slot width, align with the neighbouring icon's centre.
                    readonly property real slideOffset: slideDirection * (root.itemSize / 2 + Theme.spaceSmall) * (1 - transitionProgress)
                    enabled: !exiting
                    onExitingChanged: {
                        if (!presenceReady)
                            return;
                        if (exiting) {
                            menuOpen = false;
                            if (root.tooltipOwner === dockButton)
                                root.dismissTooltip();
                        }
                        animatePresence(exiting ? 0 : 1);
                    }
                    function animatePresence(destination) {
                        // Keep the direction when reversing an unfinished transition.
                        if (transitionProgress === 1)
                            slideDirection = dockItems.count < 2 ? 0 : index < (dockItems.count - 1) / 2 ? 1 : -1;
                        presenceAnimation.stop();
                        entering = destination === 1;
                        presenceAnimation.from = transitionProgress;
                        presenceAnimation.to = destination;
                        presenceAnimation.start();
                    }
                    function publishRectangle() {
                        if (appMenu.visible)
                            appMenu.updateAnchor();
                        const position = dockIcon.mapToItem(root.contentItem, 0, 0);
                        const rect = root.visible
                            ? Qt.rect(Math.round(position.x), Math.round(position.y), dockIcon.width, dockIcon.height)
                            : Qt.rect(0, 0, 0, 0);
                        for (const window of currentGroup.windows) {
                            if (window && typeof window.setRectangle === "function")
                                window.setRectangle(root, rect);
                        }
                    }
                    function animateLaunch() { launchOverlay.play(dockIcon); }
                    readonly property bool playingAudio: dockIcon.playingAudio
                    readonly property var appNotifications: dockIcon.appNotifications
                    readonly property int notificationCount: dockIcon.notificationCount
                    readonly property string tooltipText: dockIcon.tooltipText + (root.launchFailures[currentGroup.id] ? " - Could not open; click to retry" : "")
                    property bool active: {
                        for (let index = 0; index < currentGroup.windows.length; index++) {
                            if (currentGroup.windows[index] && currentGroup.windows[index].activated)
                                return true;

                        }
                        return false;
                    }

                    Layout.preferredWidth: root.itemSize * dockButton.transitionProgress
                    Layout.preferredHeight: root.itemSize
                    Layout.rightMargin: index === root.pinnedGroupCount - 1 ? dockRow.sectionGap : 0
                    z: root.draggedId === modelData.id ? 2 : entering || exiting ? 0 : 1
                    transform: [
                        Translate {
                            x: root.draggedId === dockButton.currentGroup.id
                                ? (root.settlingDrag && root.settlingId === dockButton.currentGroup.id ? root.settleOffset : root.dragOffset)
                                : 0
                        },
                        ReorderSlide {
                            id: reorderTransform
                            x: root.liveReorderShift(dockButton.index, dockButton.currentGroup.id)
                            animate: !root.committingReorder
                        }
                    ]
                    transformOrigin: Item.Center
                    scale: root.draggedId === dockButton.currentGroup.id ? 1.06 : 1
                    Behavior on scale {
                        enabled: !dockButton.entering && !dockButton.exiting
                        NumberAnimation {
                            duration: Theme.motion
                            easing.type: Easing.OutCubic
                        }
                    }
                    // Finish fading during the first third of the slide, before
                    // the departing icon reaches its neighbour.
                    readonly property real presenceOpacity: exiting
                        ? Math.max(0, (transitionProgress - 0.7) / 0.3) : transitionProgress
                    opacity: presenceOpacity * (root.draggedId.length > 0 && root.draggedId !== modelData.id ? 0.65 : 1)
                    Behavior on opacity {
                        enabled: !dockButton.entering && !dockButton.exiting
                        NumberAnimation {
                            duration: Theme.motion
                            easing.type: Easing.OutCubic
                        }
                    }
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: currentGroup.desktopEntry ? currentGroup.desktopEntry.name : currentGroup.id
                    Accessible.description: (playingAudio ? "Playing audio. " : "") + (notificationCount > 0 ? notificationCount + " notifications" : "")
                    Keys.onReturnPressed: root.toggleGroup(currentGroup)
                    Keys.onSpacePressed: root.toggleGroup(currentGroup)
                    Keys.onLeftPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index - 1) }
                    Keys.onRightPressed: event => { if (event.modifiers & Qt.ControlModifier) root.moveGroup(modelData.id, index + 1) }
                    NumberAnimation {
                        id: presenceAnimation
                        target: dockButton
                        property: "transitionProgress"
                        from: 0
                        to: 1
                        duration: Theme.reducedMotion ? 0 : Theme.motion * 3
                        easing.type: dockButton.exiting ? Easing.InOutCubic : Easing.OutCubic
                        onFinished: {
                            dockButton.entering = false;
                            if (dockButton.exiting)
                                Qt.callLater(root.finishGroupExit, dockButton.currentGroup.id);
                        }
                    }
                    Component.onCompleted: {
                        presenceReady = true;
                        // Entry motion belongs to this delegate's lifetime,
                        // not to later title/window-count model updates.
                        if (currentGroup.entering) {
                            dockButton.entering = true;
                            slideDirection = root.appGroups.length < 2 ? 0 : index < (root.appGroups.length - 1) / 2 ? 1 : -1;
                            transitionProgress = 0;
                            animatePresence(1);
                        }
                    }
                    Timer {
                        id: tooltipDelay
                        interval: Theme.tooltipDelay
                        running: dockMouse.containsMouse && root.draggedId.length === 0 && !dockButton.menuOpen && !dockButton.entering
                        onTriggered: {
                            root.showTooltip(dockButton);
                        }
                    }

                    Item {
                        id: buttonVisual
                        width: root.itemSize
                        height: root.itemSize
                        x: (dockButton.width - width) / 2 + dockButton.slideOffset
                        anchors.verticalCenter: parent.verticalCenter

                        Rectangle {
                            radius: Theme.insetRadius(dockSurface.radius, dockSurface.itemPadding)
                            color: dockMouse.pressed ? Theme.pressed : dockMouse.containsMouse || dockButton.activeFocus || dockButton.menuOpen ? Theme.hover : dockButton.active ? Theme.elevated : "transparent"

                            anchors {
                                fill: parent
                                margins: 0
                            }
                        }

                        AppIcon {
                            id: dockIcon
                            objectName: "dockApplicationIcon"
                            additionalBadges: [failureBadge, pinBadge]
                            presentation: DesktopLayout.presentation(root.preferences, "app:" + root.pinIdentity(dockButton.currentGroup.desktopEntry?.id || dockButton.currentGroup.id), "dock", dockButton.currentGroup.desktopEntry?.name || dockButton.currentGroup.id, dockButton.currentGroup.desktopEntry?.icon || "application-x-executable", true, false)
                            implicitSize: root.iconSize
                            opacity: root.launchFailures[dockButton.currentGroup.id] ? 0.5 : 1
                            layer.enabled: !!root.launchFailures[dockButton.currentGroup.id]
                            layer.effect: MultiEffect { saturation: -1 }
                            group: dockButton.currentGroup
                            activeStreams: root.activity.activeStreams
                            notificationIndex: root.notificationIndex

                            anchors {
                                centerIn: parent
                            }

                        }

                        DockBadge {
                            id: failureBadge
                            anchors.left: dockIcon.left
                            anchors.bottom: dockIcon.bottom
                            shown: !!root.launchFailures[dockButton.currentGroup.id]
                            iconName: "dialog-warning-symbolic"
                            color: Theme.danger
                            foreground: "#161616"
                        }

                        DockBadge {
                            id: pinBadge
                            objectName: "dockPinPreview"
                            anchors.left: dockIcon.left
                            anchors.top: dockIcon.top
                            anchors.margins: -Theme.spaceSmall
                            shown: root.draggedId === dockButton.currentGroup.id && root.dropPinned
                                && !root.isPinned(dockButton.currentGroup)
                            iconName: "view-pin-symbolic"
                            color: dockIcon.accentColor
                            foreground: dockIcon.accentForeground
                        }

                        DockWindowIndicators {
                            id: windowIndicators
                            launching: !!root.launchAttempts[dockButton.currentGroup.id]
                            windows: dockButton.currentGroup.windows
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: -3
                            anchors.horizontalCenter: parent.horizontalCenter
                        }

                    }

                    MouseArea {
                        id: dockMouse

                        anchors.fill: parent
                        anchors.topMargin: -dockSurface.itemPadding
                        anchors.bottomMargin: -(dockSurface.itemPadding + dockSurface.bottomGap)
                        enabled: !dockButton.entering
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                        cursorShape: Qt.ArrowCursor
                        hoverEnabled: true
                        onEntered: {
                            root.tooltipEntered(dockButton);
                            notificationPreview.prepare();
                        }
                        onExited: {
                            tooltipDelay.stop();
                            root.leaveTooltip(dockButton);
                        }
                        property double lastScroll: 0
                        property real pressX: 0
                        property bool moved: false
                        onPressed: function(mouse) { pressX = mouse.x; moved = false; root.dismissTooltip() }
                        onPositionChanged: function(mouse) {
                            if (!(pressedButtons & Qt.LeftButton)) return;
                            const offset = mouse.x - pressX + (root.draggedId === dockButton.currentGroup.id ? root.dragOffset : 0);
                            if (!moved && Math.abs(offset) < 8) return;
                            moved = true;
                            root.draggedId = dockButton.currentGroup.id;
                            root.dragOffset = offset;
                            root.dropIndex = root.dragDestination(dockButton.currentGroup.id, offset);
                        }
                        onReleased: {
                            if (moved)
                                root.finishDrag();
                            else
                                root.cancelDrag();
                        }
                        onCanceled: root.cancelDrag()
                        onClicked: function(mouse) {
                            if (moved) return;
                            root.applicationInteracted();
                            if (mouse.button === Qt.LeftButton) {
                                const action = root.preferences.dockClick || "toggle";
                                if (action === "launch") root.launch(dockButton.currentGroup, true);
                                else if (action === "focus") {
                                    const window = root.preferredWindow(dockButton.currentGroup);
                                    if (window) { window.minimized = false; window.activate(); }
                                    else root.launch(dockButton.currentGroup, false);
                                } else root.toggleGroup(dockButton.currentGroup);
                            } else if (mouse.button === Qt.MiddleButton) {
                                const action = root.preferences.dockMiddleClick || "launch";
                                if (action === "launch") root.launch(dockButton.currentGroup, true);
                                else if (action === "close") for (const window of dockButton.currentGroup.windows) window.close();
                            }
                            else if (mouse.button === Qt.RightButton) {
                                if (dockButton.menuOpen || menuPrepare.running) {
                                    menuPrepare.stop();
                                    dockButton.menuOpen = false;
                                } else {
                                    notificationPreview.prepare();
                                    menuPrepare.restart();
                                }
                            }
                        }
                        onWheel: function(wheel) {
                            if (root.preferences.dockScroll === "none") { wheel.accepted = false; return; }
                            const delta = wheel.pixelDelta.y || wheel.angleDelta.y || wheel.pixelDelta.x || wheel.angleDelta.x;
                            if (!delta) return;
                            const now = Date.now();
                            if (now - lastScroll < 140) return;
                            lastScroll = now;
                            root.cycleGroup(dockButton.currentGroup, delta * (root.preferences.dockScrollDirection === "reverse" ? -1 : 1));
                        }
                    }

                    // Let card creation, text layout and the parent layout settle
                    // while the surface is hidden, before starting its reveal.
                    Timer {
                        id: menuPrepare
                        interval: 32
                        onTriggered: if (!dockButton.exiting && !root.draggedId) {
                            notificationPreview.layoutCards();
                            dockButton.menuOpen = true;
                        }
                    }

                    ShellPopup {
                        id: appMenu
                        objectName: "dockAppMenu"
                        Timer {
                            id: closeClickGuard
                            interval: 350
                            onTriggered: appMenu.dismissOnOutsideClick = true
                        }
                        readonly property var mediaPlayers: Mpris.players.values.filter(player => MediaMatch.matches(player, dockButton.currentGroup))
                        revealOriginY: popupHeight
                        popupWidth: mediaPlayers.length > 0 || dockButton.notificationCount > 0 || notificationPreview.renderedNotificationCount > 0 ? 320 : 280
                        Behavior on popupWidth {
                            enabled: appMenu.visible
                            NumberAnimation { duration: Theme.reducedMotion ? 0 : 240; easing.type: Easing.OutCubic }
                        }
                        contentPadding: Theme.gap
                        popupHeight: menuColumn.implicitHeight + contentPadding * 2
                        Behavior on popupHeight {
                            enabled: appMenu.visible && notificationPreview.activeCollapses === 0
                            NumberAnimation { duration: Theme.reducedMotion ? 0 : 240; easing.type: Easing.OutCubic }
                        }
                        screen: root.screen
                        property real dockCentreX: 0
                        function updateAnchor() {
                            dockCentreX = dockButton.mapToItem(root.contentItem, dockButton.width / 2, 0).x;
                        }
                        preferredX: root.margins.left + dockCentreX - popupWidth / 2
                        preferredY: height - root.dockTopFromBottom - popupHeight - Theme.gap
                        onVisibleChanged: {
                            if (visible) {
                                updateAnchor();
                                menuNavigation.focusMenu();
                            }
                        }

                        MenuNavigator {
                            id: menuNavigation
                            entries: menuColumn.children
                            focusTarget: menuColumn
                            onEscapeRequested: dockButton.menuOpen = false
                            onActivateRequested: entry => entry.triggered()
                        }

                        Rectangle {
                            id: menuSurface

                            width: parent.width
                            height: implicitHeight
                            implicitHeight: menuColumn.implicitHeight + 12
                            color: "transparent"

                            ColumnLayout {
                                id: menuColumn

                                // Keep the container keyboard-active when the
                                // menu opens. Once an action is focused its
                                // own handlers delegate to the same navigator.
                                Keys.priority: Keys.BeforeItem
                                Keys.onDownPressed: function(event) {
                                    menuNavigation.move(1);
                                    event.accepted = true;
                                }
                                Keys.onUpPressed: function(event) {
                                    menuNavigation.move(-1);
                                    event.accepted = true;
                                }
                                Keys.onReturnPressed: function(event) {
                                    menuNavigation.activateCurrent();
                                    event.accepted = true;
                                }
                                Keys.onSpacePressed: function(event) {
                                    menuNavigation.activateCurrent();
                                    event.accepted = true;
                                }
                                Keys.onEscapePressed: function(event) {
                                    menuNavigation.escapeRequested();
                                    event.accepted = true;
                                }

                                spacing: 2

                                anchors {
                                    left: parent.left
                                    right: parent.right
                                }

                                Repeater {
                                    model: appMenu.mediaPlayers
                                    delegate: DockMediaControls {
                                        accent: dockIcon.accentColor
                                        accentForeground: dockIcon.accentForeground
                                        required property var modelData
                                        Layout.fillWidth: true
                                        player: modelData
                                        menuActive: appMenu.visible || appMenu.closing
                                        cornerRadius: appMenu.contentRadius
                                    }
                                }
                                MenuSeparator { visible: appMenu.mediaPlayers.length > 0 }

                                DockNotifications {
                                    id: notificationPreview
                                    Layout.fillWidth: true
                                    visible: entries.length > 0 || renderedNotificationCount > 0
                                    state: root.notificationStore
                                    menuActive: appMenu.visible
                                    entries: dockButton.appNotifications
                                }
                                MenuSeparator { visible: notificationPreview.visible; implicitHeight: Theme.gap * notificationPreview.presence }

                                MenuAction {
                                    objectName: "dockPinAction"
                                    cornerRadius: appMenu.contentRadius
                                    navigation: menuNavigation
                                    label: root.isPinned(dockButton.currentGroup) ? "Unpin from dock" : "Pin to dock"
                                    // Keep pinning in the same plain action group as
                                    // Launch new…; this action intentionally has no icon.
                                    iconSource: ""
                                    enabled: dockButton.currentGroup.desktopEntry !== null || root.isPinned(dockButton.currentGroup)
                                    onTriggered: {
                                        dockButton.menuOpen = false;
                                        root.setPinned(dockButton.currentGroup, !root.isPinned(dockButton.currentGroup));
                                    }
                                }

                                MenuAction {
                                    cornerRadius: appMenu.contentRadius
                                    navigation: menuNavigation
                                    label: "Launch new..."
                                    visible: dockButton.currentGroup.desktopEntry !== null
                                    onTriggered: {
                                        root.launch(dockButton.currentGroup, true);
                                        dockButton.menuOpen = false;
                                    }
                                }


                                MenuSeparator {
                                    visible: desktopActions.count > 0
                                }

                                MenuSection {
                                    label: dockButton.currentGroup.desktopEntry?.name || dockButton.currentGroup.id
                                    visible: desktopActions.count > 0
                                }

                                Repeater {
                                    id: desktopActions

                                    model: dockButton.currentGroup.windows.length > 0 && dockButton.currentGroup.desktopEntry
                                        ? dockButton.currentGroup.desktopEntry.actions : []

                                    delegate: MenuAction {
                                    cornerRadius: appMenu.contentRadius
                                        navigation: menuNavigation
                                        required property var modelData

                                        label: root.menuLabel(modelData.name, "Application action")
                                        onTriggered: {
                                            modelData.execute();
                                            dockButton.menuOpen = false;
                                        }
                                    }

                                }

                                MenuSeparator {
                                    visible: dockButton.currentGroup.windows.length > 0
                                }

                                MenuSection {
                                    label: "Open windows"
                                    visible: dockButton.currentGroup.windows.length > 0
                                }

                                Repeater {
                                    model: ScriptModel { values: dockButton.currentGroup.windows }

                                    delegate: MenuAction {
                                    cornerRadius: appMenu.contentRadius
                                        navigation: menuNavigation
                                        required property var modelData

                                        label: root.menuLabel(modelData && modelData.title, dockButton.currentGroup.id)
                                        iconSource: dockIcon.source
                                        closable: !!modelData
                                        selectedWindow: !!modelData && (modelData.activated
                                            || (appMenu.visible && !ToplevelManager.activeToplevel && root.lastActiveWindow === modelData))
                                        onCloseRequested: {
                                            if (modelData) {
                                                appMenu.dismissOnOutsideClick = false;
                                                closeClickGuard.restart();
                                                const window = modelData;
                                                Qt.callLater(() => { if (window) window.close(); });
                                            }
                                        }
                                        onTriggered: {
                                            if (modelData)
                                                modelData.activate();
                                            dockButton.menuOpen = false;
                                        }
                                    }

                                }

                            }

                        }

                    }

                }

            }
            GridLayout {
                id: dockWidgets
                // Keep a populated section visible while its first child is laid out.
                // Hiding it from its own zero implicit width prevents that first layout.
                visible: (root.preferences.layout?.dock.length || 0) > 0
                rows: 1
                columnSpacing: Theme.barControlGap
                Layout.alignment: Qt.AlignVCenter
            }


        }
        }

    }

    Connections {
        function onObjectInsertedPost(object, index) {
            root.associatePendingLaunchToplevel(object);
            root.refreshAppGroups();
        }

        function onObjectRemovedPost(object, index) {
            root.windowFocusHistory = root.windowFocusHistory.filter(window => window && window !== object);
            root.minimisedWindowHistory = root.minimisedWindowHistory.filter(window => window && window !== object);
            const remainingTargets = Object.assign({}, root.groupRestoreTargets);
            for (const id of Object.keys(remainingTargets)) {
                if (!remainingTargets[id] || remainingTargets[id] === object)
                    delete remainingTargets[id];
            }
            root.groupRestoreTargets = remainingTargets;
            root.removeToplevelAssociation(object);
            if (root.pendingLaunchToplevel === object) {
                root.pendingLaunchToplevel = null;
                root.pendingLaunchGroupId = "";
                pendingLaunchTimer.stop();
            }
            root.refreshAppGroups();
        }

        target: ToplevelManager.toplevels
    }

    Connections {
        function onApplicationsChanged() {
            root.desktopEntryCache = new Map();
            root.refreshAppGroups();
        }

        target: DesktopEntries
    }

    component MenuAction: ActionButton {
        id: action
        readonly property bool menuEntry: true
        property url iconSource: ""
        property bool closable: false
        property bool closing: false
        Timer { id: closeRetry; interval: 10000; onTriggered: action.closing = false }
        property bool selectedWindow: false
        signal closeRequested()
        property var navigation: null
        showFocusRing: navigation !== null && navigation.keyboardNavigation && activeFocus
        flat: true
        required property string label
        signal triggered()
        text: label
        leftPadding: Theme.padding
        rightPadding: closable ? Theme.spaceSmall : Theme.padding
        Layout.fillWidth: true
        implicitHeight: 38
        Keys.priority: Keys.BeforeItem
        Keys.onDownPressed: function(event) {
            if (navigation) {
                navigation.move(1);
                event.accepted = true;
            }
        }
        Keys.onUpPressed: function(event) {
            if (navigation) {
                navigation.move(-1);
                event.accepted = true;
            }
        }
        Keys.onEscapePressed: function(event) {
            if (navigation) {
                navigation.escapeRequested();
                event.accepted = true;
            }
        }
        onClicked: {
            if (navigation)
                navigation.pointerActivate();
            triggered();
        }
        contentItem: Item {
            RowLayout {
                anchors.fill: parent
                spacing: Theme.gap
                OsIconImage {
                    visible: action.iconSource.toString().length > 0
                    source: action.iconSource
                    implicitSize: Theme.iconSize
                    Layout.alignment: Qt.AlignVCenter
                }
                Text {
                    Layout.fillWidth: true
                    text: action.label
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: action.selectedWindow ? Theme.accent : Theme.text
                    font.weight: action.selectedWindow ? Font.Medium : Font.Normal
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    verticalAlignment: Text.AlignVCenter
                }
                ActionButton {
                    id: closeWindow
                    objectName: "closeWindowButton"
                    horizontalPadding: Theme.spaceSmall
                    visible: action.closable
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                    Layout.alignment: Qt.AlignVCenter
                    flat: true
                    hoverEnabled: true
                    enabled: !action.closing
                    contentItem: Item {
                        Text {
                            anchors.centerIn: parent
                            visible: !action.closing
                            text: "\u00d7"
                            color: Theme.text
                            font.pixelSize: 22
                        }
                        PreviewSpinner {
                            anchors.centerIn: parent
                            implicitWidth: 22
                            implicitHeight: 22
                            loading: action.closing
                            colour: Theme.text
                            Accessible.name: "Closing " + action.label
                        }
                    }
                    Accessible.name: (action.closing ? "Closing " : "Close ") + action.label
                    showFocusRing: action.navigation !== null && action.navigation.keyboardNavigation
                    background: Rectangle {
                        radius: closeWindow.cornerRadius
                        // The menu row is already hovered underneath this
                        // button, so use a brighter surface for its own state.
                        color: closeWindow.down ? Qt.lighter(Theme.pressed, 1.2)
                            : closeWindow.hovered ? Theme.pressed : "transparent"
                        border.width: closeWindow.showFocusRing && closeWindow.activeFocus ? 2 : 0
                        border.color: Theme.accent
                    }
                    onClicked: {
                        if (action.navigation)
                            action.navigation.pointerActivate();
                        action.closing = true;
                        closeRetry.restart();
                        action.closeRequested();
                    }
                }
            }
        }
    }

    component MenuSection: Item {
        required property string label
        implicitHeight: sectionLabel.implicitHeight + Theme.spaceSmall
        Layout.fillWidth: true

        Text {
            id: sectionLabel
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            leftPadding: Theme.padding
            rightPadding: Theme.padding
            text: parent.label
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
    }

    component MenuSeparator: Item {
        implicitHeight: Theme.gap
        Layout.fillWidth: true

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: Theme.padding
            anchors.rightMargin: Theme.padding
            height: 1
            color: Theme.outline
            opacity: 0.8
        }
    }
}
