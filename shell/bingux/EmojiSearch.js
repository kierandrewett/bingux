const aliases = {
    lol: "laugh", happy: "smile", sad: "cry", love: "heart", thanks: "folded hands",
    yes: "thumbs up", no: "thumbs down", party: "party popper", fire: "fire",
    cool: "sunglasses", uk: "united kingdom", usa: "united states"
};

function foldSkinTones(rows) {
    const groups = new Map();
    for (const row of rows) {
        let name = row.name.replace(/(?:medium-light|medium-dark|light|medium|dark) skin tone/g, "")
            .replace(/([:,])(?:,|\s)+/g, "$1 ").replace(/[:, ]+$/, "");
        // Unicode uses expanded person sequences for mixed-tone couples.
        name = name.replace(/^(kiss|couple with heart): person, person$/, "$1");
        let group = groups.get(name);
        if (!group) {
            group = Object.assign({}, row, {name, tones: [], variants: []});
            groups.set(name, group);
        }
        group.variants.push(row.emoji);
        const modifiers = (row.emoji.match(/\ud83c[\udffb-\udfff]/g) || []);
        if (!modifiers.length) group.tones[0] = row.emoji;
        else if (modifiers.every(c => c === modifiers[0]))
            group.tones[modifiers[0].charCodeAt(1) - 0xdffa] = row.emoji;
    }
    return Array.from(groups.values());
}

function search(rows, query, group, recent, skinTone = 0) {
    const display = row => row.tones ? Object.assign({}, row, {emoji: row.tones[skinTone] || row.tones[0] || row.emoji}) : row;
    const words = String(query || "").toLocaleLowerCase().replace(/[:_]/g, " ").trim().split(/\s+/).filter(Boolean);
    if (!words.length && group === "recent") {
        const byEmoji = new Map();
        for (const row of rows) for (const emoji of row.variants || [row.emoji]) byEmoji.set(emoji, row);
        return Array.from(new Set(recent.map(emoji => byEmoji.get(emoji)).filter(Boolean))).map(display);
    }
    const result = rows.filter(row => (!group || group === "recent" || row.group === group)
        && words.every(word => {
            const haystack = (row.name + " " + row.keywords + " " + row.emoji).toLocaleLowerCase();
            return haystack.includes(word) || (aliases[word] && haystack.includes(aliases[word]));
        }));
    if (words.length) {
        const exact = words.join(" ");
        result.sort((a, b) => Number(b.name.toLocaleLowerCase() === exact) - Number(a.name.toLocaleLowerCase() === exact)
            || a.name.length - b.name.length);
    }
    return result.map(display);
}

function remember(recent, emoji) {
    return [emoji].concat(recent.filter(value => value !== emoji)).slice(0, 32);
}
