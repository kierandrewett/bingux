.import "ControlLayout.js" as ControlLayout

// `contexts` lists the session contexts a widget may appear in. "desktop" is
// the running shell; "locked" is the lock screen; "greeter" is the login
// screen; "kiosk" is a pre-login host such as the OS installer. Widgets
// without the field are desktop-only. Pre-login contexts show only what makes
// sense before a user session exists; SessionServices gates the actions those
// widgets would take, and StatusBar keeps the slots of desktop-only widgets
// that the default layout places beside shared ones, shown disabled.
const sessionContexts = ["desktop", "locked", "greeter", "kiosk"];
const widgets = [
    { id: "search", label: "Search", icon: "system-search-symbolic", contexts: ["desktop"] },
    { id: "workspaces", label: "Workspaces", icon: "view-grid-symbolic", contexts: ["desktop"] },
    {
        id: "clock",
        label: "Date and time",
        icon: "x-office-calendar-symbolic",
        contexts: ["desktop", "locked", "greeter", "kiosk"],
    },
    {
        id: "controls",
        label: "Control centre",
        icon: "preferences-system-symbolic",
        contexts: ["desktop", "locked", "greeter", "kiosk"],
    },
    {
        id: "notifications",
        label: "Notifications",
        icon: "preferences-system-notifications-symbolic",
        contexts: ["desktop", "locked"],
    },
    {
        id: "metrics",
        label: "System monitors",
        icon: "computer-symbolic",
        contexts: ["desktop", "locked", "greeter", "kiosk"],
    },
    {
        id: "keyboard",
        label: "Keyboard layout",
        icon: "input-keyboard-symbolic",
        contexts: ["desktop", "locked", "greeter", "kiosk"],
    },
    { id: "tray", label: "System tray", icon: "view-more-symbolic", contexts: ["desktop"] },
    { id: "overflow", label: "More status controls", icon: "view-more-symbolic", contexts: ["desktop"] },
    {
        id: "privacy",
        label: "Privacy",
        icon: "microphone-sensitivity-high-symbolic",
        contexts: ["desktop", "locked"],
    },
    { id: "capture", label: "Recording", icon: "media-record-symbolic", contexts: ["desktop"] },
    { id: "terminal", label: "Terminal", icon: "utilities-terminal-symbolic", panel: true, contexts: ["desktop"] },
    { id: "notes", label: "Notes", icon: "accessories-text-editor-symbolic", panel: true, contexts: ["desktop"] },
    { id: "monitor", label: "System", icon: "computer-symbolic", panel: true, contexts: ["desktop"] },
    { id: "calendar", label: "Calendar", icon: "x-office-calendar-symbolic", panel: true, contexts: ["desktop"] },
    { id: "media", label: "Media", icon: "applications-multimedia-symbolic", panel: true, contexts: ["desktop"] },
    { id: "tasks", label: "Tasks", icon: "view-list-symbolic", panel: true, contexts: ["desktop"] },
];
// True when widget `id` may be shown in session context `mode`. Spacing and
// decoration items follow the bar they sit in; control-centre tiles,
// portable widgets and extensions are desktop-only.
function allowed(id, mode) {
    if (mode === "desktop") return true;
    if (!sessionContexts.includes(mode)) return false;
    if (isSpacing(id) || isDecoration(id)) return true;
    const item = widgets.find((w) => w.id === id);
    return !!item && (item.contexts || ["desktop"]).includes(mode);
}
const layoutWidgets = [
    { id: "spacer", label: "Space", layoutItem: true },
    { id: "spring", label: "Flexible space", layoutItem: true, flexible: true },
];
function isSpacing(id) {
    return /^(spacer|spring)(:[1-9][0-9]{0,3})?$/.test(id);
}
function freshInstanceId(layout, kind) {
    const used = Object.values(layout).reduce((items, values) => items.concat(values), []);
    for (let index = 1; index <= 9999; index++) if (!used.includes(kind + ":" + index)) return kind + ":" + index;
    return "";
}
const decorationWidgets = [
    { id: "label", label: "Label", icon: "font-x-generic-symbolic", decoration: true },
    { id: "icon", label: "Icon", icon: "starred-symbolic", decoration: true },
];
function isDecoration(id) {
    return /^(label|icon)(:[1-9][0-9]{0,3})?$/.test(id);
}
function isTemplate(id) {
    return ["spacer", "spring", "label", "icon"].includes(id);
}
const controlWidgets = [
    { id: "control-network", label: "Wi-Fi", icon: "network-wireless-symbolic" },
    { id: "control-bluetooth", label: "Bluetooth", icon: "bluetooth-active-symbolic" },
    { id: "control-vpn", label: "VPN", icon: "network-vpn-symbolic" },
    { id: "control-dnd", label: "Do Not Disturb", icon: "notifications-disabled-symbolic" },
    { id: "control-nightLight", label: "Night Light", icon: "night-light-symbolic" },
    { id: "control-power", label: "Power mode", icon: "power-profile-balanced-symbolic" },
    { id: "control-awake", label: "Keep Awake", icon: "display-brightness-symbolic" },
];
function controlOrder() {
    return controlWidgets.map((item) => item.id.slice(8));
}
function presentation(desktop, id, container, label, icon, nativeIcon, nativeText, parentContainer) {
    const options = desktop.widgetOptions?.[id] || {};
    const inherited =
        desktop.containers?.[container]?.display || desktop.containers?.[parentContainer]?.display || "native";
    const mode = !options.display || options.display === "inherit" ? inherited : options.display;
    return {
        mode,
        label: options.label || label,
        icon: options.icon || icon,
        labelOverridden: !!options.label,
        iconOverridden: !!options.icon,
        showIcon: mode === "native" ? nativeIcon : mode !== "text",
        showText: mode === "native" ? nativeText : mode !== "icons",
        custom: mode !== "native" || !!options.label || !!options.icon,
    };
}
function defaults() {
    return {
        "top-left": ["search"],
        "top-center": ["clock"],
        "top-right": ["capture", "tray", "privacy", "metrics", "keyboard", "overflow", "controls", "notifications"],
        dock: [],
        sidebar: ["terminal", "notes", "monitor", "calendar", "media", "tasks"],
    };
}
function widget(id) {
    if (ControlLayout.isExtension(id)) return { id, label: id.split("/").pop(), extension: true };
    if (isSpacing(id))
        return Object.assign(
            {},
            layoutWidgets.find((item) => item.id === id.split(":")[0]),
            { id },
        );
    if (isDecoration(id))
        return Object.assign(
            {},
            decorationWidgets.find((item) => item.id === id.split(":")[0]),
            { id },
        );
    return (
        widgets.concat(controlWidgets).find((w) => w.id === id) ||
        (ControlLayout.isPortable(id) ? ControlLayout.widget(id) : undefined)
    );
}
function placement(desktop, id) {
    return (
        zone(desktop.layout || {}, id) ||
        (ControlLayout.isExternal(id) && ControlLayout.contains(desktop.controlLayout, "control-centre", id)
            ? "control-centre"
            : "")
    );
}
function zone(layout, id) {
    return Object.keys(layout).find((key) => layout[key].includes(id)) || "";
}
// A drop is positioned against visible neighbours, but saved orders include hidden widgets.
function insertionIndex(order, draggedId, visibleIds, before) {
    const remaining = order.filter((id) => id !== draggedId);
    const next = before >= 0 ? remaining.indexOf(visibleIds[before]) : -1;
    if (next >= 0) return next;
    const previous = remaining.indexOf(visibleIds[visibleIds.length - 1]);
    return previous >= 0 ? previous + 1 : remaining.length;
}
function accepts(id, target) {
    const item = widget(id);
    if (item?.layoutItem) return ["top-left", "top-center", "top-right", "palette"].includes(target);
    if (id.startsWith("control-") || ControlLayout.isContainer(id))
        return (
            !!item &&
            ["control-centre", "top-left", "top-center", "top-right", "dock", "sidebar", "palette"].includes(target)
        );
    return (
        !!item &&
        ["palette", "top-left", "top-center", "top-right", "dock", "control-centre", "sidebar"].includes(target)
    );
}
function move(layout, id, target, index) {
    if (!accepts(id, target)) return layout;
    if (target !== "palette" && !(target in layout)) return layout;
    if (isTemplate(id)) {
        if (target === "palette") return layout;
        id = freshInstanceId(layout, id);
        if (!id) return layout;
    }
    const next = {};
    for (const key of Object.keys(layout)) next[key] = layout[key].filter((value) => value !== id);
    if (target !== "palette") next[target].splice(Math.max(0, Math.min(index, next[target].length)), 0, id);
    return next;
}
