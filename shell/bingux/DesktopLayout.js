const widgets = [
    {id: "search", label: "Search", icon: "system-search-symbolic"},
    {id: "clock", label: "Date and time", icon: "x-office-calendar-symbolic"},
    {id: "controls", label: "Control centre", icon: "preferences-system-symbolic"},
    {id: "notifications", label: "Notifications", icon: "preferences-system-notifications-symbolic"},
    {id: "metrics", label: "System monitors", icon: "computer-symbolic"},
    {id: "keyboard", label: "Keyboard layout", icon: "input-keyboard-symbolic"},
    {id: "tray", label: "System tray", icon: "view-more-symbolic"},
    {id: "overflow", label: "More status controls", icon: "view-more-symbolic"},
    {id: "privacy", label: "Privacy", icon: "microphone-sensitivity-high-symbolic"},
    {id: "capture", label: "Recording", icon: "media-record-symbolic"},
    {id: "terminal", label: "Terminal", icon: "utilities-terminal-symbolic", panel: true},
    {id: "notes", label: "Notes", icon: "accessories-text-editor-symbolic", panel: true},
    {id: "monitor", label: "System", icon: "computer-symbolic", panel: true},
    {id: "calendar", label: "Calendar", icon: "x-office-calendar-symbolic", panel: true},
    {id: "media", label: "Media", icon: "applications-multimedia-symbolic", panel: true},
    {id: "tasks", label: "Tasks", icon: "view-list-symbolic", panel: true}
];
const controlWidgets = [
    {id: "control-network", label: "Wi-Fi", icon: "network-wireless-symbolic"},
    {id: "control-bluetooth", label: "Bluetooth", icon: "bluetooth-active-symbolic"},
    {id: "control-vpn", label: "VPN", icon: "network-vpn-symbolic"},
    {id: "control-dnd", label: "Do Not Disturb", icon: "notifications-disabled-symbolic"},
    {id: "control-nightLight", label: "Night Light", icon: "night-light-symbolic"},
    {id: "control-power", label: "Power mode", icon: "power-profile-balanced-symbolic"},
    {id: "control-awake", label: "Keep Awake", icon: "display-brightness-symbolic"}
];
function controlOrder() { return controlWidgets.map(item => item.id.slice(8)); }
function defaults() {
    return {"top-left": ["search"], "top-center": ["clock"],
        "top-right": ["capture", "tray", "privacy", "metrics", "keyboard", "overflow", "controls", "notifications"],
        dock: [], sidebar: ["terminal", "notes", "monitor", "calendar", "media", "tasks"]};
}
function widget(id) { return widgets.concat(controlWidgets).find(w => w.id === id); }
function zone(layout, id) { return Object.keys(layout).find(key => layout[key].includes(id)) || ""; }
function accepts(id, target) {
    const item = widget(id);
    if (id.startsWith("control-")) return !!item && ["control-centre", "palette"].includes(target);
    return !!item && (target === "palette" || (item.panel ? target === "sidebar" : ["top-left", "top-center", "top-right", "dock"].includes(target)));
}
function move(layout, id, target, index) {
    if (!accepts(id, target)) return layout;
    if (target !== "palette" && !(target in layout)) return layout;
    if (target !== "sidebar" && id === layout.sidebar[0] && layout.sidebar.length === 1) return layout;
    const next = {};
    for (const key of Object.keys(layout)) next[key] = layout[key].filter(value => value !== id);
    if (target !== "palette") next[target].splice(Math.max(0, Math.min(index, next[target].length)), 0, id);
    return next;
}
