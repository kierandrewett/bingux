function normalize(value) {
    return String(value || "").replace(/\.desktop$/i, "").toLowerCase();
}

// Icon lookup is presentation-only. Playback matching below uses exact IDs
// and an exact name fallback only for the known generic Chromium bus.
function playerDesktopEntry(player, provider) {
    if (!player) return null;
    const desktopId = String(player.desktopEntry || "").replace(/\.desktop$/i, "");
    function lookup(id) {
        return id ? provider.byId(id) || provider.byId(id + ".desktop")
            || provider.heuristicLookup(id) : null;
    }
    if (desktopId) return lookup(desktopId);
    // Browser forks may publish a generic Chromium bus name but their real
    // application name as Identity. Let the OS app provider resolve that.
    return lookup(String(player.identity || ""))
        || lookup(String(player.dbusName || "").replace(/^org\.mpris\.MediaPlayer2\./, "")
            .replace(/\.instance[^.]*$/, ""));
}

function matchesIdentity(value, group) {
    return matches({ desktopEntry: value }, group);
}

function matchesAudio(properties, group) {
    if (!properties || !group) return false;
    const desktopId = properties["application.id"] || properties["application.desktop"];
    if (desktopId) return matchesIdentity(desktopId, group);
    // A process identity outranks generic Electron/Chromium display names.
    const binary = properties["application.process.binary"];
    if (binary) return matchesIdentity(String(binary).split("/").pop(), group);
    const name = properties["application.name"] || properties["node.name"];
    return !!name && (matchesIdentity(name, group)
        || normalize(name) === normalize((group.desktopEntry || {}).name));
}

function matchesNotification(entry, group) {
    if (!entry || !group) return false;
    if (entry.desktopEntry) return matchesIdentity(entry.desktopEntry, group);
    // Older senders omit desktop-entry. Only accept an exact app name.
    return !!entry.appName && (matchesIdentity(entry.appName, group)
        || normalize(entry.appName) === normalize((group.desktopEntry || {}).name));
}

function matches(player, group) {
    if (!player || !group) return false;
    const entry = group.desktopEntry || {};
    const identities = [group.id, entry.id, entry.startupClass]
        .concat((group.windows || []).map(window => window.appId))
        .map(normalize).filter(Boolean);
    // DesktopEntry identifies the app. Do not override it with a track title,
    // a human-readable player name, or another application's bus name.
    const desktopId = normalize(player.desktopEntry);
    if (desktopId) return identities.includes(desktopId);
    const busId = normalize(String(player.dbusName || "")
        .replace(/^org\.mpris\.MediaPlayer2\./, "").replace(/\.instance[^.]*$/, ""));
    // Chromium forks omit DesktopEntry and share Chromium's bus prefix.
    // Only that known generic bus permits an exact application-name fallback.
    if (busId === "chromium" && normalize(player.identity)) {
        const identity = normalize(player.identity);
        return identities.includes(identity) || identity === normalize(entry.name);
    }
    return busId.length > 0 && identities.includes(busId);
}

function timeLabel(seconds) {
    const total = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
    const minutes = Math.floor(total / 60);
    return minutes + ":" + String(total % 60).padStart(2, "0");
}

// Duration is in seconds. A browser alone does not identify video: web music
// players should keep track controls unless the current item is long-form.
function prefersSeeking(player) {
    if (!player) return false;
    if (player.lengthSupported && player.length > 600) return true;
    const metadata = player.metadata || {};
    if (/^video\//i.test(String(metadata["xesam:contentType"] || ""))) return true;
    const url = String(metadata["xesam:url"] || "");
    if (/^https?:\/\/(?:www\.|m\.)?(?:youtube\.com\/(?:watch|shorts|live)|youtu\.be\/|vimeo\.com\/|twitch\.tv\/|netflix\.com\/|disneyplus\.com\/)/i.test(url)) return true;
    return ["mpv", "celluloid", "io.github.celluloid_player.celluloid", "totem", "org.gnome.totem", "org.gnome.showtime"]
        .includes(normalize(player.desktopEntry));
}

function canStep(player, direction) {
    if (!player || !player.canControl) return false;
    return prefersSeeking(player) ? !!player.canSeek : !!(direction < 0 ? player.canGoPrevious : player.canGoNext);
}

function step(player, direction) {
    if (!canStep(player, direction)) return;
    if (prefersSeeking(player)) {
        let offset = direction < 0 ? -10 : 10;
        if (player.positionSupported) {
            offset = Math.max(-player.position, offset);
            if (player.lengthSupported && player.length > 0)
                offset = Math.min(offset, Math.max(0, player.length - player.position));
        }
        player.seek(offset);
    } else if (direction < 0) player.previous();
    else player.next();
}
