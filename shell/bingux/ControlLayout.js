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

const widgets = [
    {id: "controls-header", label: "Account and session", group: true},
    {id: "controls-audio", label: "Audio controls", group: true},
    {id: "control-divider", label: "Divider", group: true},
    {id: "controls-tiles", label: "Quick controls", group: true},
    {id: "control-media", label: "Media player", group: true},
    {id: "control-customise", label: "Customise button", group: true},
    {id: "control-account", action: true, label: "User account", icon: "avatar-default-symbolic"},
    {id: "control-header-space", label: "Header space", group: true},
    {id: "control-battery", label: "Battery", icon: "battery-good-symbolic", group: true},
    {id: "control-settings", action: true, label: "Settings", icon: "org.gnome.Settings-symbolic"},
    {id: "control-session", action: true, label: "Power options", icon: "system-shutdown-symbolic"},
    {id: "control-lock", action: true, label: "Lock", icon: "system-lock-screen-symbolic"},
    {id: "control-volume", label: "Volume", icon: "audio-volume-high-symbolic", group: true},
    {id: "control-microphone", label: "Microphone", icon: "audio-input-microphone-symbolic", group: true}
];
function widget(id) { return widgets.find(item => item.id === id); }
function groupFor(id) { return Object.keys(defaults().groups).find(group => items(defaults(), group).includes(id)) || ""; }

function isAction(id) { return widget(id)?.action === true; }

function isPortable(id) { return isAction(id) || audio.includes(id) || ["control-battery", "control-media"].includes(id); }

function validPlacement(desktop) {
    if (!desktop.layout) return true;
    const lists = Object.values(desktop.layout);
    if (!lists.every(items => Array.isArray(items))) return false;
    const placed = lists.reduce((all, items) => all.concat(items), []).filter(isPortable);
    return !placed.length || (!!desktop.controlLayout && placed.every(id => !contains(desktop.controlLayout, groupFor(id), id)));
}
