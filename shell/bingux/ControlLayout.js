// Native groups remain distinct so importing a layout does not flatten its UI.
const sections = ["controls-header", "controls-audio", "control-divider", "controls-tiles", "control-media", "control-customise"];
const header = ["control-account", "control-header-space", "control-battery", "control-settings", "control-session", "control-lock"];
const audio = ["control-volume", "control-microphone"];
function defaults() {
    return {version: 1, groups: {
        "control-centre": sections.slice(),
        "controls-header": header.slice(),
        "controls-audio": audio.slice()
    }};
}
function valid(layout) {
    if (!layout || layout.version !== 1 || !layout.groups || Array.isArray(layout.groups)) return false;
    if (Object.keys(layout).sort().join() !== "groups,version") return false;
    const nativeGroups = defaults().groups;
    if (Object.keys(layout.groups).sort().join() !== Object.keys(nativeGroups).sort().join()) return false;
    return Object.keys(nativeGroups).every(group => Array.isArray(layout.groups[group]) &&
        layout.groups[group].every((id, index, values) => typeof id === "string" && nativeGroups[group].includes(id) && values.indexOf(id) === index));
}
function items(layout, group) { return (layout || defaults()).groups[group] || []; }
function position(layout, group, id) { return items(layout, group).indexOf(id); }
function contains(layout, group, id) { return position(layout, group, id) >= 0; }
function move(layout, group, id, index) {
    const current = layout || defaults();
    if (!items(defaults(), group).includes(id)) return current;
    const next = JSON.parse(JSON.stringify(current));
    const order = next.groups[group].filter(value => value !== id);
    if (index >= 0) order.splice(Math.max(0, Math.min(index, order.length)), 0, id);
    next.groups[group] = order;
    return next;
}
