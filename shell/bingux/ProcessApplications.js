// Keep ambiguous aliases unresolved, as the desktop-entry scan did.
function index(entries) {
    const aliases = new Map();
    for (const entry of entries) {
        const keys = new Set(
            [
                entry.id,
                entry.name,
                entry.startupClass,
                String(entry.command?.[0] || "")
                    .split("/")
                    .pop(),
            ]
                .map((value) =>
                    String(value || "")
                        .replace(/\.desktop$/i, "")
                        .toLowerCase(),
                )
                .filter(Boolean),
        );
        for (const key of keys) aliases.set(key, aliases.has(key) ? null : entry);
    }
    return aliases;
}

function lookup(process, aliases, desktopEntries) {
    const names = [process.executable, process.name].filter(Boolean);
    for (const name of names) {
        const exact = desktopEntries.byId(name) || desktopEntries.byId(name + ".desktop");
        if (exact) return exact;
        const match = aliases.get(name.toLowerCase());
        if (match) return match;
    }
    return names.length ? desktopEntries.heuristicLookup(names[0]) : null;
}
