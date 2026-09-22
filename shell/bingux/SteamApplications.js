function normalizedTitle(value) {
    return String(value || "").trim().replace(/\s+/g, " ").toLowerCase();
}

function steamAppId(entry) {
    const explicitAppId = String(entry && entry.steamAppId || "").match(/^[0-9]+$/);
    if (explicitAppId)
        return explicitAppId[0];

    const idMatch = String(entry && entry.id || "").match(/^steam_app_([0-9]+)(?:\.desktop)?$/i);
    if (idMatch)
        return idMatch[1];

    const command = String(entry && entry.execString || "");
    const match = command.match(/steam:\/\/rungameid\/([0-9]+)/i);
    return match ? match[1] : "";
}

function uniqueMatch(values, predicate) {
    const matches = (values || []).filter(predicate);
    return matches.length === 1 ? matches[0] : null;
}

function uniqueTitleMatch(values, title) {
    const wantedTitle = normalizedTitle(title);
    if (!wantedTitle)
        return null;

    return uniqueMatch(values, value => normalizedTitle(value.name) === wantedTitle);
}

function steamAppIdMatch(appId) {
    const match = String(appId || "").replace(/\.desktop$/i, "").match(/^steam_app_([0-9]+)$/i);
    return match ? match[1] : "";
}

function entryForWindow(entries, appId, title) {
    const values = (entries || []).filter(entry => steamAppId(entry));
    const id = steamAppIdMatch(appId);
    return id ? uniqueMatch(values, entry => steamAppId(entry) === id) : uniqueTitleMatch(values, title);
}

function gameForWindow(games, appId, title) {
    const values = games || [];
    const id = steamAppIdMatch(appId);
    return id ? uniqueMatch(values, game => String(game.appId) === id) : uniqueTitleMatch(values, title);
}

function wrappedDesktopEntry(entry, game) {
    return {
        id: entry.id,
        startupClass: entry.startupClass,
        name: entry.name,
        icon: game.iconPath || entry.icon,
        steamAppId: String(game.appId),
        actions: entry.actions,
        command: entry.command,
        execString: entry.execString,
        workingDirectory: entry.workingDirectory,
        execute: function() {
            entry.execute();
        }
    };
}
