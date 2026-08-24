import Quickshell.Services.Notifications
import QtQuick

QtObject {
    id: root

    readonly property int maxVisibleNotifications: 3
    readonly property int maxQueuedNotifications: 32
    readonly property int defaultTimeoutMs: 5000
    readonly property int maxTimeoutMs: 30000

    property var visibleEntries: []
    property var queuedEntries: []

    function removeFrom(entries, notification) {
        const result = [];

        for (let index = 0; index < entries.length; index += 1) {
            if (entries[index].notification !== notification) {
                result.push(entries[index]);
            }
        }

        return result;
    }

    function removeReplacedNotification(notification) {
        let changed = false;
        const visible = [];
        const queued = [];

        for (let index = 0; index < visibleEntries.length; index += 1) {
            const entry = visibleEntries[index];
            if (entry.notification.id === notification.id) {
                changed = true;
            } else {
                visible.push(entry);
            }
        }

        for (let index = 0; index < queuedEntries.length; index += 1) {
            const entry = queuedEntries[index];
            if (entry.notification.id === notification.id) {
                changed = true;
            } else {
                queued.push(entry);
            }
        }

        if (changed) {
            visibleEntries = visible;
            queuedEntries = queued;
        }
    }

    function promoteNextNotification() {
        if (visibleEntries.length >= maxVisibleNotifications || queuedEntries.length === 0) {
            return;
        }

        const next = queuedEntries[0];
        queuedEntries = queuedEntries.slice(1);
        visibleEntries = visibleEntries.concat([next]);
    }

    function scheduleExpiry() {
        const entries = visibleEntries.concat(queuedEntries);
        let nextDeadline = 0;

        for (let index = 0; index < entries.length; index += 1) {
            const deadline = entries[index].deadline;
            if (deadline > 0 && (nextDeadline === 0 || deadline < nextDeadline)) {
                nextDeadline = deadline;
            }
        }

        if (nextDeadline === 0) {
            expiryTimer.stop();
            return;
        }

        expiryTimer.interval = Math.max(1, nextDeadline - Date.now());
        expiryTimer.restart();
    }

    function expireDueNotifications() {
        const now = Date.now();
        const entries = visibleEntries.concat(queuedEntries);
        const expired = [];

        for (let index = 0; index < entries.length; index += 1) {
            if (entries[index].deadline > 0 && entries[index].deadline <= now) {
                expired.push(entries[index].notification);
            }
        }

        for (let index = 0; index < expired.length; index += 1) {
            expire(expired[index]);
        }

        scheduleExpiry();
    }

    function accept(notification) {
        removeReplacedNotification(notification);
        notification.tracked = true;
        notification.closed.connect(function(_reason) {
            root.remove(notification);
        });

        const timeout = timeoutFor(notification);
        const entry = {
            notification: notification,
            deadline: timeout > 0 ? Date.now() + timeout : 0,
        };

        if (visibleEntries.length < maxVisibleNotifications) {
            visibleEntries = [entry].concat(visibleEntries);
        } else if (queuedEntries.length >= maxQueuedNotifications) {
            notification.expire();
        } else {
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

        if (wasVisible) {
            promoteNextNotification();
        }

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
        if (notification.expireTimeout > 0) {
            return Math.min(Math.round(notification.expireTimeout * 1000), maxTimeoutMs);
        }

        return notification.resident ? 0 : defaultTimeoutMs;
    }

    property var expiryTimer: Timer {
        repeat: false
        onTriggered: root.expireDueNotifications()
    }

    NotificationServer {
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
