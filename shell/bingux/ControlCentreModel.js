// Presentation policy shared by the detail page and its checks.
function connectionIcon(connection) {
    if (connection.type === "802-11-wireless") return "network-wireless-symbolic";
    if (["vpn", "wireguard", "tun"].includes(connection.type)) return "network-vpn-symbolic";
    return "network-wired-symbolic";
}
function connectionName(connection) {
    if (connection.connected && connection.type === "802-3-ethernet") return "Ethernet";
    if (connection.name === "tailscale0") return "Tailscale";
    return connection.name;
}
function networkSections(connections, wireless) {
    const physical = connections.filter(connection => ["802-3-ethernet", "802-11-wireless", "gsm", "cdma", "bluetooth"].includes(connection.type));
    const current = physical.filter(connection => connection.connected);
    const nearby = wireless.filter(network => !network.connected && !current.some(connection => connection.type === "802-11-wireless" && connection.name === network.name));
    const other = physical.filter(connection => !current.includes(connection));
    return {current: current, nearby: nearby, other: other};
}

function bluetoothIcon(icon) {
    const name = String(icon || "").replace(/-symbolic$/, "");
    const devices = ["audio-headset", "audio-headphones", "audio-card", "audio-speakers",
        "input-keyboard", "input-mouse", "input-gaming", "input-tablet", "input-touchpad",
        "computer", "phone", "tablet", "printer", "camera-photo", "camera-video"];
    return devices.includes(name) ? name + "-symbolic" : "bluetooth-active-symbolic";
}
