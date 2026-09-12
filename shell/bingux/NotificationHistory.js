// Durable presentation data. D-Bus actions belong to a live server connection.
function encode(entries, session) {
    return JSON.stringify({
        version: 1,
        entries: entries.map((entry) => ({
            key: entry.historyKey || session + ":" + entry.notification.id,
            session: session,
            sourceId: entry.notification.id,
            receivedAt: entry.receivedAt,
            appName: entry.appName,
            desktopEntry: entry.desktopEntry,
            appIcon: entry.appIcon,
            summary: entry.summary,
            body: entry.body,
            image: String(entry.image || "").startsWith("image:") ? "" : entry.image || "",
        })),
    });
}
function restore(text, current, session) {
    let data;
    try {
        data = JSON.parse(text || "{}");
    } catch (_) {
        return current;
    }
    if (data.version !== 1 || !Array.isArray(data.entries)) return current;
    const entries = current.slice();
    const knownKeys = new Set(entries.map((entry) => entry.historyKey));
    let nextId = entries.reduce((minimum, entry) => Math.min(minimum, entry.notification.id), 0) - 1;
    for (const record of data.entries) {
        if (
            !record ||
            typeof record.key !== "string" ||
            typeof record.summary !== "string" ||
            knownKeys.has(record.key)
        )
            continue;
        knownKeys.add(record.key);
        const id = record.session === session && record.sourceId > 0 ? record.sourceId : nextId--;
        const notification = {
            id: id,
            appName: record.appName || "",
            desktopEntry: record.desktopEntry || "",
            appIcon: record.appIcon || "",
            summary: record.summary,
            body: record.body || "",
            actions: [],
            expireTimeout: 0,
            lastGeneration: true,
            dismiss: function () {},
        };
        entries.push({
            notification: notification,
            historyKey: record.key,
            restoredFromDisk: true,
            appName: notification.appName,
            desktopEntry: notification.desktopEntry,
            appIcon: notification.appIcon,
            summary: notification.summary,
            body: notification.body,
            image: record.image || "",
            receivedAt: Number(record.receivedAt) || 0,
            toastVisible: false,
            timeoutMs: 0,
            deadline: 0,
            remainingMs: 0,
            paused: false,
            actions: [],
        });
    }
    return entries.sort((a, b) => b.receivedAt - a.receivedAt);
}
function largeImage(entry) {
    return (
        /\b(screenshot|screen capture|screen recording)\b/i.test((entry.appName || "") + " " + (entry.summary || "")) ||
        /(?:^|\.)(spectacle|flameshot|gnome-screenshot|capture)$/.test(entry.desktopEntry || "")
    );
}
