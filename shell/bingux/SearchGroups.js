.pragma library

function key(result) {
    if (result.providerId === "web" || result.providerId === "web-suggestions" || result.providerId === "web-shortcuts") return "web";
    if (result.kind === "application") return "applications";
    if (result.kind === "file" || result.kind === "folder") return "files";
    if (result.kind === "calculation") return "calculation";
    if (result.kind === "weather") return "weather";
    if (result.kind === "chat") return "ai";
    return result.providerId;
}

function label(group) {
    const labels = {
        applications: "Applications", files: "Files", web: "Web",
        calculation: "Calculator", weather: "Weather", ai: "Assistant"
    };
    return labels[group] || group.split("-").map(word => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
}

function ordered(results) {
    const order = ["calculation", "weather", "ai", "applications", "files"];
    return results.map((result, index) => ({ result: result, index: index })).sort((left, right) => {
        const a = key(left.result), b = key(right.result);
        // QML's JS runtime need not provide a stable sort. Preserve the
        // incoming relevance order explicitly, including equal-score rows.
        if (a === b) return left.index - right.index;
        const rank = group => group === "web" ? 100 : order.includes(group) ? order.indexOf(group) : 50;
        return rank(a) - rank(b) || a.localeCompare(b);
    }).map(entry => entry.result);
}
