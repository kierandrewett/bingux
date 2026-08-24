import QtQuick
import Quickshell.Services.Notifications

QtObject {
    id: root

    readonly property int maxVisibleNotifications: 3
    readonly property int maxQueuedNotifications: 32
    readonly property int defaultTimeoutMs: 5000
    readonly property int maxTimeoutMs: 30000
    property var visibleEntries: []
    property var queuedEntries: []
    property var watchedNotifications: []
    property var expiryTimer
    property var notificationServer

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
        const replacement = {
            "notification": notification,
            "deadline": timeout > 0 ? Date.now() + timeout : 0
        };
        let replaced = false;
        const visible = [];
        const queued = [];
        for (let index = 0; index < visibleEntries.length; index += 1) {
            const entry = visibleEntries[index];
            if (entry.notification.id === notification.id) {
                if (!replaced) {
                    visible.push(replacement);
                    replaced = true;
                }
            } else {
                visible.push(entry);
            }
        }
        for (let index = 0; index < queuedEntries.length; index += 1) {
            const entry = queuedEntries[index];
            if (entry.notification.id === notification.id) {
                if (!replaced) {
                    queued.push(replacement);
                    replaced = true;
                }
            } else {
                queued.push(entry);
            }
        }
        if (!replaced)
            return false;

        visibleEntries = visible;
        queuedEntries = queued;
        return true;
    }

    function resetExpiry(notification) {
        const timeout = timeoutFor(notification);
        const deadline = timeout > 0 ? Date.now() + timeout : 0;
        let changed = false;
        const visible = [];
        const queued = [];
        for (let index = 0; index < visibleEntries.length; index += 1) {
            const entry = visibleEntries[index];
            if (entry.notification === notification) {
                visible.push({
                    "notification": entry.notification,
                    "deadline": deadline
                });
                changed = true;
            } else {
                visible.push(entry);
            }
        }
        for (let index = 0; index < queuedEntries.length; index += 1) {
            const entry = queuedEntries[index];
            if (entry.notification === notification) {
                queued.push({
                    "notification": entry.notification,
                    "deadline": deadline
                });
                changed = true;
            } else {
                queued.push(entry);
            }
        }
        if (changed) {
            visibleEntries = visible;
            queuedEntries = queued;
            scheduleExpiry();
        }
    }

    function unwatchNotification(notification) {
        const watched = [];
        let changed = false;
        for (let index = 0; index < watchedNotifications.length; index += 1) {
            if (watchedNotifications[index] === notification)
                changed = true;
            else
                watched.push(watchedNotifications[index]);
        }
        if (changed)
            watchedNotifications = watched;

    }

    function watchNotification(notification) {
        for (let index = 0; index < watchedNotifications.length; index += 1) {
            if (watchedNotifications[index] === notification)
                return ;

        }
        watchedNotifications = watchedNotifications.concat([notification]);
        const reset = function reset() {
            root.resetExpiry(notification);
        };
        const changeSignals = ["expireTimeoutChanged", "appNameChanged", "appIconChanged", "summaryChanged", "bodyChanged", "urgencyChanged", "actionsChanged", "hasActionIconsChanged", "residentChanged", "transientChanged", "desktopEntryChanged", "imageChanged", "hasInlineReplyChanged", "inlineReplyPlaceholderChanged", "hintsChanged"];
        for (let index = 0; index < changeSignals.length; index += 1) {
            const signal = notification[changeSignals[index]];
            if (signal && signal.connect)
                signal.connect(reset);

        }
        notification.closed.connect(function(_reason) {
            root.unwatchNotification(notification);
            root.remove(notification);
        });
    }

    function promoteNextNotification() {
        if (visibleEntries.length >= maxVisibleNotifications || queuedEntries.length === 0)
            return ;

        const next = queuedEntries[0];
        queuedEntries = queuedEntries.slice(1);
        visibleEntries = visibleEntries.concat([next]);
    }

    function scheduleExpiry() {
        const entries = visibleEntries.concat(queuedEntries);
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
        const entries = visibleEntries.concat(queuedEntries);
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
        if (replaceExistingNotification(notification)) {
            watchNotification(notification);
            scheduleExpiry();
            return ;
        }
        const timeout = timeoutFor(notification);
        const entry = {
            "notification": notification,
            "deadline": timeout > 0 ? Date.now() + timeout : 0
        };
        if (visibleEntries.length < maxVisibleNotifications) {
            watchNotification(notification);
            visibleEntries = [entry].concat(visibleEntries);
        } else if (queuedEntries.length >= maxQueuedNotifications) {
            notification.expire();
        } else {
            watchNotification(notification);
            queuedEntries = queuedEntries.concat([entry]);
        }
        scheduleExpiry();
    }

    function remove(notification) {
        const wasVisible = visibleEntries.some(function(entry) {
            return entry.notification === notification;
        });
        visibleEntries = removeFrom(visibleEntries, notification);
        queuedEntries = removeFrom(queuedEntries, notification);
        if (wasVisible)
            promoteNextNotification();

        scheduleExpiry();
    }

    function dismiss(notification) {
        remove(notification);
        notification.dismiss();
    }

    function expire(notification) {
        remove(notification);
        notification.expire();
    }

    function timeoutFor(notification) {
        if (notification.expireTimeout === 0)
            return 0;

        if (notification.expireTimeout > 0)
            return Math.min(Math.round(notification.expireTimeout * 1000), maxTimeoutMs);

        return defaultTimeoutMs;
    }

    expiryTimer: Timer {
        repeat: false
        onTriggered: root.expireDueNotifications()
    }

    notificationServer: NotificationServer {
        bodyImagesSupported: false
        bodyMarkupSupported: false
        bodyHyperlinksSupported: false
        actionsSupported: true
        actionIconsSupported: false
        imageSupported: false
        inlineReplySupported: false
        persistenceSupported: false
        keepOnReload: false
        onNotification: function(notification) {
            root.accept(notification);
        }
    }

}
