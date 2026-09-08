import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "NotificationHistory.js" as History
import Quickshell.Services.Notifications

Scope {
    id: root

    readonly property int maxApplicationNameLength: 128
    readonly property int maxIconNameLength: 256
    readonly property int maxSummaryLength: 256
    readonly property int maxBodyLength: 2048
    readonly property int maxActionTextLength: 128
    readonly property int maxActionsPerNotification: 8
    readonly property int maxScannedActions: 32
    readonly property int minTimeoutMs: 4000
    readonly property int maxTimeoutMs: 20000
    property bool doNotDisturb: false
    onDoNotDisturbChanged: if (doNotDisturb) archiveToasts()
    property var allEntries: []
    readonly property var visibleEntries: allEntries.filter(entry => entry.toastVisible)
    property var notificationWatchers: []
    property var expiryTimer
    property var desktopEntryWatcher
    readonly property alias retainedState: notificationMetadata
    property var notificationServer
    property bool historyReady: false
    property string historyDirectory: Quickshell.statePath("notifications")
    onAllEntriesChanged: if (historyReady) historySave.restart()
    Process {
        id: historyDirectorySetup
        command: ["mkdir", "-p", "-m", "700", root.historyDirectory]
        running: true
        onExited: code => { if (code === 0) historyFile.path = root.historyDirectory + "/history.json"; }
    }
    FileView {
        id: historyFile
        printErrors: false
        atomicWrites: true
        onLoaded: root.loadHistory(text())
        onLoadFailed: root.loadHistory("")
        onSaveFailed: console.warn("Could not save notification history")
    }
    Timer { id: historySave; interval: 40; onTriggered: root.saveHistory() }
    Component.onDestruction: if (historyReady) { historyFile.blockWrites = true; saveHistory(); }

    function loadHistory(text) {
        if (historyReady) return;
        allEntries = History.restore(text, allEntries, retainedState.sessionToken);
        historyReady = true;
        saveHistory();
    }
    function saveHistory() {
        if (historyReady) historyFile.setText(History.encode(allEntries, retainedState.sessionToken));
    }
    function cacheImage(entry, item) {
        if (!historyReady || item.opacity < 1 || !entry.image || String(entry.image).includes(historyDirectory)) return;
        const key = entry.historyKey || String(entry.notification.id);
        const path = historyDirectory + "/" + key.replace(/[^a-zA-Z0-9_-]/g, "_") + ".png";
        item.grabToImage(result => {
            if (!result.saveToFile(path)) return;
            allEntries = allEntries.map(current => current.historyKey === key
                ? Object.assign({}, current, {image: "file://" + path}) : current);
        });
    }
    function boundedText(value, maxLength) {
        const text = String(value || "").replace(/[\u0000-\u001f\u007f]/g, " ").trim();
        if (text.length <= maxLength)
            return text;

        return text.slice(0, Math.max(0, maxLength - 3)) + "...";
    }

    function projectActions(notification) {
        const sourceActions = notification.actions || [];
        const projected = [];
        let defaultAction = null;
        const actionCount = Math.min(sourceActions.length, maxScannedActions);
        for (let index = 0; index < actionCount; index += 1) {
            const action = sourceActions[index];
            const entry = {
                "action": action,
                "defaultAction": action.identifier === "default",
                "text": boundedText(action.text, maxActionTextLength)
            };
            if (entry.defaultAction) {
                if (defaultAction === null)
                    defaultAction = entry;
            } else if (projected.length < maxActionsPerNotification) {
                projected.push(entry);
            }
        }

        if (defaultAction !== null)
            projected.unshift(defaultAction);

        return projected;
    }

    function applicationFor(notification) {
        const identity = boundedText(notification.desktopEntry || notification.appName, maxApplicationNameLength)
            .replace(/\.desktop$/, "").toLowerCase();
        if (!identity) return null;
        const desktopId = boundedText(notification.desktopEntry, maxApplicationNameLength);
        const direct = desktopId ? DesktopEntries.byId(desktopId.replace(/\.desktop$/, ""))
            || DesktopEntries.byId(desktopId) : null;
        if (direct) return direct;
        // Electron variants can advertise their window class instead of their desktop ID.
        const matches = DesktopEntries.applications.values.filter(application =>
            [application.id, application.startupClass, application.name].some(value =>
                String(value || "").replace(/\.desktop$/, "").toLowerCase() === identity));
        return matches.length === 1 ? matches[0] : null;
    }

    function activationTarget(entry) {
        const application = applicationFor(entry);
        const identities = [entry.desktopEntry, application ? application.id : "", application ? application.startupClass : ""]
            .map(value => String(value || "").replace(/\.desktop$/, "").toLowerCase()).filter(Boolean);
        const windows = ToplevelManager.toplevels.values.filter(window =>
            identities.includes(String(window.appId || "").replace(/\.desktop$/, "").toLowerCase()));
        return {application: application, window: windows.find(window => window.activated) || windows[0]};
    }

    function canActivate(entry) {
        if (entry.actions.some(action => action.defaultAction && action.action && typeof action.action.invoke === "function"))
            return true;
        const target = activationTarget(entry);
        return !!(target.window || target.application);
    }

    function activate(entry) {
        const action = entry.actions.find(action => action.defaultAction && action.action && typeof action.action.invoke === "function");
        const target = activationTarget(entry);
        if (action) {
            // Focus first. The sender can then select a more specific window
            // when it handles the action, without our fallback overriding it.
            if (target.window) target.window.activate();
            action.action.invoke();
            return true;
        }
        if (target.window) {
            target.window.activate();
        } else if (target.application) {
            const helper = Quickshell.env("BINGUX_APP_LAUNCHER_HELPER");
            const command = helper ? [helper] : ["python3", decodeURIComponent(Qt.resolvedUrl("launch-application.py").toString().replace(/^file:\/\//, ""))];
            Quickshell.execDetached(command.concat(["--notify-errors", "--", target.application.id]));
        } else {
            return false;
        }
        archive(entry.notification);
        return true;
    }

    function notificationImage(notification) {
        const hints = notification.hints || {};
        // Keep raw pixels authoritative. File icons must reach the shared SVG
        // renderer before Qt's notification provider decodes them.
        if (hints["image-data"] || hints["image_data"] || hints["icon_data"])
            return notification.image || "";
        const path = hints["image-path"] || hints["image_path"];
        return typeof path === "string" && path ? path : notification.image || "";
    }

    function entryFor(notification, deadline, previous) {
        const application = applicationFor(notification);
        const toastVisible = !root.doNotDisturb && (previous ? previous.toastVisible : !(notification.lastGeneration && JSON.parse(retainedState.hiddenIdsJson || "{}")[String(notification.id)]));
        const paused = toastVisible && previous ? previous.paused : false;
        return {
            "notification": notification,
            "historyKey": previous && previous.historyKey || retainedState.sessionToken + ":" + notification.id,
            "toastVisible": toastVisible,
            "receivedAt": JSON.parse(retainedState.receivedTimesJson)[String(notification.id)] || Date.now(),
            "timeoutMs": timeoutFor(notification),
            "deadline": paused || !toastVisible ? 0 : deadline,
            "paused": paused,
            "remainingMs": toastVisible && deadline > 0 ? Math.max(1, deadline - Date.now()) : 0,
            "appName": boundedText(application ? application.name : notification.appName, maxApplicationNameLength),
            "desktopEntry": boundedText(notification.desktopEntry, maxApplicationNameLength),
            "appIcon": boundedText((application ? application.icon : "") || notification.appIcon, maxIconNameLength),
            "summary": boundedText(notification.summary, maxSummaryLength),
            "body": boundedText(notification.body, maxBodyLength),
            "image": notificationImage(notification),
            "actions": projectActions(notification)
        };
    }

    function refreshApplicationMetadata() {
        let changed = false;
        const refreshed = allEntries.map(entry => {
            const application = applicationFor(entry.notification);
            const appName = boundedText(application ? application.name : entry.notification.appName, maxApplicationNameLength);
            const appIcon = boundedText((application ? application.icon : "") || entry.notification.appIcon, maxIconNameLength);
            if (appName === entry.appName && appIcon === entry.appIcon) return entry;
            changed = true;
            return Object.assign({}, entry, {appName: appName, appIcon: appIcon});
        });
        if (changed) allEntries = refreshed;
    }

    // Keep the model stable while the pointer is over a card, including its controls.
    function setPaused(notification, paused) {
        const entry = visibleEntries.find(entry => entry.notification === notification);
        if (!entry || entry.paused === paused)
            return;
        entry.paused = paused;
        if (paused) {
            entry.remainingMs = entry.deadline > 0 ? Math.max(1, entry.deadline - Date.now()) : 0;
            entry.deadline = 0;
        } else {
            entry.deadline = entry.remainingMs > 0 ? Date.now() + entry.remainingMs : 0;
        }
        scheduleExpiry();
    }

    function expiryProgress(notification) {
        const entry = visibleEntries.find(entry => entry.notification === notification);
        if (!entry)
            return 1;
        if (entry.timeoutMs <= 0)
            return 0;
        const remaining = entry.paused ? entry.remainingMs : Math.max(0, entry.deadline - Date.now());
        return Math.max(0, Math.min(1, 1 - remaining / entry.timeoutMs));
    }

    function removeFrom(entries, notification) {
        const result = [];
        for (let index = 0; index < entries.length; index += 1) {
            if (entries[index].notification !== notification)
                result.push(entries[index]);

        }
        return result;
    }

    function replaceExistingNotification(notification) {
        const timeout = timeoutFor(notification);
        const replacement = entryFor(notification, timeout > 0 ? Date.now() + timeout : 0);
        const replacedNotifications = [];
        let replaced = false;
        const visible = [];
        for (let index = 0; index < allEntries.length; index += 1) {
            const entry = allEntries[index];
            if (entry.notification.id === notification.id) {
                if (entry.notification !== notification)
                    replacedNotifications.push(entry.notification);
                if (!replaced) {
                    visible.push(entryFor(notification, replacement.deadline, notification.lastGeneration ? entry : Object.assign({}, entry, {toastVisible: true})));
                    replaced = true;
                }
            } else {
                visible.push(entry);
            }
        }
        if (!replaced)
            return false;

        allEntries = visible;
        for (let index = 0; index < replacedNotifications.length; index += 1)
            unwatchNotification(replacedNotifications[index]);

        return true;
    }

    function resetExpiry(notification) {
        const timeout = timeoutFor(notification);
        const deadline = timeout > 0 ? Date.now() + timeout : 0;
        let changed = false;
        const visible = [];
        for (let index = 0; index < allEntries.length; index += 1) {
            const entry = allEntries[index];
            if (entry.notification === notification) {
                visible.push(entryFor(entry.notification, deadline, entry));
                changed = true;
            } else {
                visible.push(entry);
            }
        }
        if (changed) {
            allEntries = visible;
            scheduleExpiry();
        }
    }

    function unwatchNotification(notification) {
        const retained = [];
        let changed = false;
        for (let index = 0; index < notificationWatchers.length; index += 1) {
            const watcher = notificationWatchers[index];
            if (watcher.notification !== notification) {
                retained.push(watcher);
                continue;
            }

            changed = true;
            for (let signalIndex = 0; signalIndex < watcher.resetSignals.length; signalIndex += 1) {
                const signal = watcher.resetSignals[signalIndex];
                if (signal && signal.disconnect)
                    signal.disconnect(watcher.reset);
            }
            if (notification.closed && notification.closed.disconnect)
                notification.closed.disconnect(watcher.closed);
        }
        if (changed)
            notificationWatchers = retained;
    }

    function watchNotification(notification) {
        for (let index = 0; index < notificationWatchers.length; index += 1) {
            if (notificationWatchers[index].notification === notification)
                return;
        }

        const reset = function reset() {
            root.resetExpiry(notification);
        };
        const closed = function closed(_reason) {
            root.remove(notification);
        };
        const watcher = {
            "closed": closed,
            "notification": notification,
            "reset": reset,
            "resetSignals": []
        };
        notificationWatchers = notificationWatchers.concat([watcher]);

        const changeSignals = ["expireTimeoutChanged", "appNameChanged", "appIconChanged", "summaryChanged", "bodyChanged", "urgencyChanged", "actionsChanged", "hasActionIconsChanged", "residentChanged", "transientChanged", "desktopEntryChanged", "imageChanged", "hasInlineReplyChanged", "inlineReplyPlaceholderChanged", "hintsChanged"];
        for (let index = 0; index < changeSignals.length; index += 1) {
            const signal = notification[changeSignals[index]];
            if (signal && signal.connect) {
                signal.connect(reset);
                watcher.resetSignals.push(signal);
            }
        }
        notification.closed.connect(closed);
    }

    function scheduleExpiry() {
        const entries = visibleEntries;
        let nextDeadline = 0;
        for (let index = 0; index < entries.length; index += 1) {
            const deadline = entries[index].deadline;
            if (deadline > 0 && (nextDeadline === 0 || deadline < nextDeadline))
                nextDeadline = deadline;

        }
        if (nextDeadline === 0) {
            expiryTimer.stop();
            return ;
        }
        expiryTimer.interval = Math.max(1, nextDeadline - Date.now());
        expiryTimer.restart();
    }

    function expireDueNotifications() {
        const now = Date.now();
        const entries = visibleEntries;
        const expired = [];
        for (let index = 0; index < entries.length; index += 1) {
            if (entries[index].deadline > 0 && entries[index].deadline <= now)
                expired.push(entries[index].notification);

        }
        for (let index = 0; index < expired.length; index += 1) {
            expire(expired[index]);
        }
        scheduleExpiry();
    }

    function accept(notification) {
        notification.tracked = true;
        const key = String(notification.id);
        const times = JSON.parse(retainedState.receivedTimesJson);
        if (!notification.lastGeneration) {
            const hidden = JSON.parse(retainedState.hiddenIdsJson || "{}");
            delete hidden[key];
            retainedState.hiddenIdsJson = JSON.stringify(hidden);
        }
        if (!notification.lastGeneration || !times[key]) {
            times[key] = Date.now();
            retainedState.receivedTimesJson = JSON.stringify(times);
        }
        if (replaceExistingNotification(notification)) {
            watchNotification(notification);
            scheduleExpiry();
            return ;
        }
        const timeout = timeoutFor(notification);
        const entry = entryFor(notification, timeout > 0 ? Date.now() + timeout : 0);
        watchNotification(notification);
        allEntries = [entry].concat(allEntries);
        scheduleExpiry();
    }

    function remove(notification) {
        const times = JSON.parse(retainedState.receivedTimesJson);
        delete times[String(notification.id)];
        retainedState.receivedTimesJson = JSON.stringify(times);
        unwatchNotification(notification);
        allEntries = removeFrom(allEntries, notification);
        const hidden = JSON.parse(retainedState.hiddenIdsJson || "{}");
        delete hidden[String(notification.id)];
        retainedState.hiddenIdsJson = JSON.stringify(hidden);

        scheduleExpiry();
    }

    function dismiss(notification) {
        remove(notification);
        notification.dismiss();
    }

    // Leaving the desktop is a presentation change, not NotificationClosed.
    function archive(notification) {
        const hidden = JSON.parse(retainedState.hiddenIdsJson || "{}");
        hidden[String(notification.id)] = true;
        retainedState.hiddenIdsJson = JSON.stringify(hidden);
        allEntries = allEntries.map(entry => entry.notification === notification
            ? Object.assign({}, entry, {toastVisible: false, deadline: 0, remainingMs: 0, paused: false}) : entry);
        scheduleExpiry();
    }

    function archiveToasts() {
        const hidden = JSON.parse(retainedState.hiddenIdsJson || "{}");
        allEntries = allEntries.map(entry => {
            hidden[String(entry.notification.id)] = true;
            return Object.assign({}, entry, {toastVisible: false, deadline: 0, remainingMs: 0, paused: false});
        });
        retainedState.hiddenIdsJson = JSON.stringify(hidden);
        scheduleExpiry();
    }

    function expire(notification) { archive(notification); }

    function dismissAll() {
        for (const entry of allEntries.slice()) dismiss(entry.notification);
    }

    function timeoutFor(notification) {
        if (notification.expireTimeout === 0)
            return 0;

        // Match the bounded plain text displayed by the card. Allow time to
        // notice it, then roughly 200 words/minute. The character estimate also
        // gives long tokens and text without spaces enough reading time.
        const text = (boundedText(notification.summary, maxSummaryLength) + " "
            + boundedText(notification.body, maxBodyLength)).replace(/\s+/g, " ").trim();
        const words = text ? text.split(" ").length : 0;
        const readingTime = 1500 + Math.max(words * 300, Array.from(text).length * 60);
        const requestedTime = notification.expireTimeout > 0 ? notification.expireTimeout * 1000 : 0;
        return Math.round(Math.min(maxTimeoutMs, Math.max(minTimeoutMs, readingTime, requestedTime)));
    }

    expiryTimer: Timer {
        repeat: false
        onTriggered: root.expireDueNotifications()
    }

    PersistentProperties {
        id: notificationMetadata
        reloadableId: "notification-metadata"
        property string sessionToken: Date.now().toString(36) + Math.random().toString(36).slice(2)
        property string receivedTimesJson: "{}"
        property string hiddenIdsJson: "{}"
        onLoaded: {
            const times = JSON.parse(receivedTimesJson);
            const hidden = JSON.parse(hiddenIdsJson);
            const restore = entry => Object.assign({}, entry, {
                receivedAt: times[String(entry.notification.id)] || entry.receivedAt,
                toastVisible: !hidden[String(entry.notification.id)] && entry.toastVisible,
                deadline: hidden[String(entry.notification.id)] ? 0 : entry.deadline
            });
            root.allEntries = root.allEntries.map(restore);
            root.scheduleExpiry();
        }
    }

    // DesktopEntries emits one change per entry during a rescan. Refresh once
    // after the batch, rather than once for every installed application.
    Timer {
        id: applicationRefresh
        interval: 100
        onTriggered: root.refreshApplicationMetadata()
    }

    desktopEntryWatcher: Connections {
        target: DesktopEntries.applications
        function onValuesChanged() { applicationRefresh.restart(); }
    }

    notificationServer: NotificationServer {
        bodyImagesSupported: false
        bodyMarkupSupported: false
        bodyHyperlinksSupported: false
        actionsSupported: true
        actionIconsSupported: false
        imageSupported: true
        inlineReplySupported: false
        persistenceSupported: true
        keepOnReload: true
        onNotification: function(notification) {
            root.accept(notification);
        }
    }

}
