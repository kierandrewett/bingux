import QtQuick
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Io

// Public command actions share the same objects and operations as the UI.
Scope {
    id: root
    required property var indicators
    required property var mediaControls
    required property var notificationState
    required property var dockView
    property var inputSelector: null
    property var applications: ApplicationCatalog.entries
    property var windowIds: new Map()
    property int nextWindowId: 0
    readonly property string instance: Date.now().toString(36)
    function ok(value) { return JSON.stringify(value || {ok: true}); }
    function fail(message) { return JSON.stringify({ok: false, error: message}); }
    function appCommand(action, identity, query) {
        if (action === "list") {
            const term = query.trim().toLowerCase();
            return ok({apps: applications.filter(entry => !term || (entry.id + " " + entry.name + " " + entry.genericName).toLowerCase().includes(term))
                .map(entry => ({id: entry.id, name: entry.name, icon: entry.icon, terminal: entry.runInTerminal}))
                .sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id))});
        }
        if (!["launch", "new-window"].includes(action)) return fail("Unknown application action");
        const id = identity.replace(/\.desktop$/, "");
        const entry = applications.find(item => item.id.replace(/\.desktop$/, "") === id);
        if (!entry) return fail("Installed application not found; use apps list");
        const group = dockView.appGroups.find(item => item.desktopEntry && item.desktopEntry.id === entry.id)
            || {id: entry.id, desktopEntry: entry, windows: []};
        dockView.launch(group, action === "new-window");
        return ok({ok: true, pending: true, id: entry.id});
    }
    function keyboardCommand(action, type, identity) {
        const selector = inputSelector;
        if (!selector) return fail("Keyboard controls are unavailable");
        if (action === "list") return ok({ready: selector.metrics.desktopStateAvailable,
            busy: selector.selectionBusy, error: selector.lastError,
            sources: selector.sources.map(source => ({type: source.type, id: source.id, name: source.displayName,
                selected: !!selector.metrics.currentInputSource && selector.sourceKey(source) === selector.sourceKey(selector.metrics.currentInputSource)}))});
        if (action !== "select") return fail("Unknown keyboard action");
        if (!selector.canSelect) return fail("Keyboard controls are unavailable or busy");
        const source = selector.sources.find(item => item.type === type && item.id === identity);
        if (!source) return fail("Keyboard source not found; use keyboard list");
        selector.selectSource(source, false);
        return ok({ok: true, pending: true});
    }
    function controlCommand(action, identity) {
        const service = mediaControls.services;
        const choices = mediaControls.controlChoices;
        if (action === "list") return ok({ready: service.preferencesReady, error: service.error,
            controls: choices.map(item => ({id: item.id, name: item.title, available: item.available, shown: service.showControl(item.id)}))});
        if (!["show", "hide"].includes(action)) return fail("Unknown control visibility action");
        if (!service.preferencesReady) return fail("Control preferences are not ready");
        const choice = choices.find(item => item.id === identity);
        if (!choice) return fail("Control not found; use controls list");
        if (action === "show" && !choice.available) return fail("This control is unavailable on this device");
        service.setControl(identity, action === "show");
        return ok({ok: true, shown: action === "show"});
    }
    function vpnCommand(action, identity) {
        const service = mediaControls.services;
        if (action === "list" || action === "status") return ok({ready: service.ready, busy: service.busy, error: service.error, connections: service.vpns});
        if (!["connect", "disconnect"].includes(action)) return fail("Unknown VPN action");
        if (!service.ready || service.busy) return fail("Desktop controls are unavailable or busy");
        const connection = service.vpns.find(item => item.id === identity);
        if (!connection || !connection.canToggle) return fail("VPN connection is unavailable; use vpn list");
        const enabled = action === "connect";
        if (connection.connected === enabled) return ok({ok: true, changed: false});
        service.action({kind: "vpn", id: identity, enabled});
        return ok({ok: true, pending: true});
    }
    function pageCommand(page, input) {
        if (!["network", "bluetooth", "audio", "display", "vpn", "power", "customise"].includes(page)) return fail("Unknown control-centre page");
        if (input && page !== "audio") return fail("Input selection requires the audio page");
        mediaControls.visible = true;
        mediaControls.openDetail(page, null, input ? "input" : "output");
        return ok({ok: true, page});
    }
    function deviceCommand(action, input, identity) {
        const details = mediaControls.deviceControls;
        const nodes = input ? details.audioInputs : details.audioOutputs;
        const selected = input ? details.inputNode : details.outputNode;
        if (action === "list") return ok({devices: nodes.map(node => ({id: node.name,
            name: node.description || node.name, selected: node === selected})), input});
        if (action !== "select") return fail("Unknown audio-device action");
        const node = nodes.find(item => item.name === identity);
        if (!node) return fail("Audio device no longer exists");
        if (input) details.inputSelected(node); else details.outputSelected(node);
        return ok({ok: true, pending: true, id: identity});
    }
    function serviceCommand(kind, action, value) {
        const service = mediaControls.services;
        const state = service.state;
        if (!["power", "night-light", "awake"].includes(kind)) return fail("Unknown desktop control");
        if (action === "status") {
            const common = {ready: service.ready, busy: service.busy, error: service.error};
            if (kind === "power") return ok(Object.assign(common, state.power || {available: false, profiles: []}));
            return ok(Object.assign(common, {available: !!state[kind === "awake" ? "awakeAvailable" : "nightLightAvailable"],
                enabled: kind === "awake" ? service.keepAwake : state.nightLight,
                active: kind === "awake" ? service.keepAwake : state.nightLightActive}));
        }
        if (!service.ready || service.busy) return fail("Desktop controls are unavailable or busy");
        if (kind === "power") {
            if (action !== "set" || !state.power?.available || !state.power.profiles.includes(value)) return fail("Power profile is unavailable");
            service.action({kind: "power", profile: value});
        } else {
            if (!["on", "off", "toggle"].includes(action)) return fail("Unknown toggle action");
            if (!state[kind === "awake" ? "awakeAvailable" : "nightLightAvailable"]) return fail("This desktop control is unavailable");
            const current = kind === "awake" ? service.keepAwake : state.nightLight;
            const enabled = action === "toggle" ? !current : action === "on";
            if (kind === "awake") { if (enabled !== current) service.toggleAwake(); }
            else service.action({kind: "nightLight", enabled});
        }
        return ok({ok: true, pending: true});
    }
    function networkCommand(action, identity) {
        const details = mediaControls.deviceControls;
        if (action === "status") return ok({ready: details.networkUpdatedAt > 0, busy: details.networkBusy,
            updatedAt: details.networkUpdatedAt, error: details.connectionError || details.networkError,
            connections: details.connections, wireless: details.wirelessNetworks});
        if (action === "refresh") {
            if (details.networkBusy) return fail("Network controls are busy; use network status to inspect progress");
            details.connectionError = "";
            details.refreshNetwork(); return ok({ok: true, pending: true}); }
        if (action === "connect" || action === "disconnect") return ok(details.connectionForCommand(identity, action === "connect" ? "up" : "down"));
        return fail("Unknown network action");
    }
    function bluetoothCommand(action, identity) {
        const adapter = mediaControls.bluetoothAdapter;
        if (!adapter) return fail("Bluetooth adapter is unavailable");
        const devices = adapter.devices.values;
        if (action === "status" || action === "list") return ok({enabled: adapter.enabled, scanning: adapter.discovering,
            devices: devices.map(device => ({id: device.address, name: device.name || device.deviceName,
                connected: device.connected, paired: device.paired, blocked: device.blocked, state: device.state}))});
        if (["on", "off", "toggle"].includes(action)) {
            adapter.enabled = action === "toggle" ? !adapter.enabled : action === "on";
        } else if (action === "scan") {
            if (!["on", "off"].includes(identity)) return fail("Scan requires on or off");
            if (!adapter.enabled && identity === "on") return fail("Enable Bluetooth before scanning");
            mediaControls.deviceControls.scanForCommand(identity === "on");
        } else if (action === "connect" || action === "disconnect") {
            if (!adapter.enabled) return fail("Bluetooth is off");
            const device = devices.find(item => item.address.toLowerCase() === identity.toLowerCase());
            if (!device) return fail("Bluetooth device not found");
            if (action === "connect" && (!device.paired || device.blocked)) return fail("Pair and unblock this device in Bluetooth settings first");
            if ([BluetoothDeviceState.Connecting, BluetoothDeviceState.Disconnecting].includes(device.state)) return fail("Device connection is busy");
            if (action === "connect" && !device.connected) device.connect();
            if (action === "disconnect" && device.connected) device.disconnect();
        } else return fail("Unknown Bluetooth action");
        return ok({ok: true, pending: true});
    }
    function audioCommand(action, input, value) {
        if (!["status", "volume", "up", "down", "mute", "unmute", "toggle"].includes(action)) return fail("Unknown audio action");
        const node = input ? indicators.audioSource : indicators.audioSink;
        if (!node || !node.ready || !node.audio) return fail("Audio device is unavailable");
        const audio = node.audio;
        if (["volume", "up", "down"].includes(action)) {
            if (!Number.isFinite(value) || value < 0 || value > 100) return fail("Volume must be between 0 and 100");
            const target = action === "volume" ? value / 100 : audio.volume + (action === "up" ? value : -value) / 100;
            audio.volume = Math.max(0, Math.min(1, target));
        } else if (action !== "status") audio.muted = action === "toggle" ? !audio.muted : action === "mute";
        return ok({volume: Math.round(audio.volume * 100), muted: audio.muted, input});
    }
    function mediaCommand(action, identity, value) {
        const players = mediaControls.mediaPlayers;
        const describe = player => ({id: player.dbusName, name: player.identity, playing: player.isPlaying,
            title: player.trackTitle, artist: player.trackArtist, position: player.positionSupported ? player.position : null,
            length: player.lengthSupported ? player.length : null});
        if (action === "list") return ok({players: players.map(describe)});
        const player = identity ? players.find(candidate => candidate.dbusName === identity) : mediaControls.mediaPlayer;
        if (!player) return fail(identity ? "Media player not found" : "No media player is available");
        if (action === "status") return ok(describe(player));
        if (!player.canControl) return fail("This player cannot be controlled");
        const verb = action === "toggle" ? (player.isPlaying ? "pause" : "play") : action;
        const capabilities = {play: "canPlay", pause: "canPause", next: "canGoNext", previous: "canGoPrevious", seek: "canSeek"};
        if (!(verb in capabilities)) return fail("Unknown media action");
        if (!player[capabilities[verb]]) return fail("This player does not support " + verb);
        if (verb === "seek") {
            if (!Number.isFinite(value) || value < 0 || !player.positionSupported
                || (player.lengthSupported && value > player.length)) return fail("Invalid playback position");
            player.position = value;
        } else player[verb]();
        return ok();
    }
    function notificationCommand(action, identity, actionId) {
        const entries = notificationState.allEntries;
        if (action === "list") return ok({notifications: entries.map(entry => ({id: entry.historyKey,
            app: entry.appName, summary: entry.summary, body: entry.body,
            actions: entry.actions.map(item => ({id: item.action.identifier, text: item.text}))}))});
        if (action === "clear") { notificationState.dismissAll(); return ok(); }
        const entry = entries.find(candidate => candidate.historyKey === identity);
        if (!entry) return fail("Notification no longer exists");
        if (action === "dismiss") notificationState.dismiss(entry.notification);
        else if (action === "invoke") {
            const selected = entry.actions.find(item => item.action.identifier === actionId);
            if (!selected || typeof selected.action.invoke !== "function") return fail("Notification action is unavailable");
            selected.action.invoke();
        } else return fail("Unknown notification action");
        return ok();
    }
    function dockCommand(action, identity, destination) {
        const groups = dockView.appGroups;
        if (action === "list") return ok({apps: groups.map((group, index) => ({id: group.id,
            name: group.desktopEntry ? group.desktopEntry.name : group.id, index,
            pinned: dockView.isPinned(group), windows: group.windows.length}))});
        const group = groups.find(candidate => candidate.id === identity);
        if (!group) return fail("Dock item no longer exists");
        if (["pin", "launch"].includes(action) && !group.desktopEntry) return fail("This app has no desktop entry");
        if (action === "pin" || action === "unpin") dockView.setPinned(group, action === "pin");
        else if (action === "launch") dockView.launch(group, true);
        else if (action === "activate") {
            const window = dockView.preferredWindow(group);
            if (window) { window.minimized = false; window.activate(); }
            else if (group.desktopEntry) dockView.launch(group, false);
            else return fail("This app cannot be launched");
        } else if (action === "move") {
            if (!Number.isInteger(destination) || destination < 0 || destination >= groups.length) return fail("Invalid dock position");
            if (dockView.isPinned(group) !== dockView.isPinned(groups[destination])) return fail("Pin or unpin the app before moving it across dock sections");
            dockView.moveGroup(group.id, destination);
        } else return fail("Unknown dock action");
        return ok();
    }
    function windowRecords() {
        const windows = [];
        for (const group of dockView.appGroups) for (const window of group.windows) {
            if (!window || windows.some(record => record.window === window)) continue;
            if (!windowIds.has(window)) windowIds.set(window, instance + ":" + (++nextWindowId));
            windows.push({window, id: windowIds.get(window), appId: group.id});
        }
        const active = new Set(windows.map(record => record.window));
        for (const window of windowIds.keys()) if (!active.has(window)) windowIds.delete(window);
        return windows;
    }
    function windowCommand(action, identity) {
        const records = windowRecords();
        if (action === "list") return ok({windows: records.map(record => ({id: record.id, appId: record.appId,
            title: record.window.title, active: record.window.activated, minimized: record.window.minimized}))});
        const record = records.find(candidate => candidate.id === identity);
        if (!record) return fail("Window no longer exists; refresh the window list after shell reloads");
        if (action === "activate") { record.window.minimized = false; record.window.activate(); }
        else if (action === "minimize") record.window.minimized = true;
        else if (action === "restore") record.window.minimized = false;
        else if (action === "close") record.window.close();
        else return fail("Unknown window action");
        return ok();
    }
    IpcHandler {
        target: "actions"
        function app(action: string, id: string, query: string): string { return root.appCommand(action, id, query); }
        function keyboard(action: string, type: string, id: string): string { return root.keyboardCommand(action, type, id); }
        function control(action: string, id: string): string { return root.controlCommand(action, id); }
        function vpn(action: string, id: string): string { return root.vpnCommand(action, id); }
        function page(name: string, input: bool): string { return root.pageCommand(name, input); }
        function device(action: string, input: bool, id: string): string { return root.deviceCommand(action, input, id); }
        function service(kind: string, action: string, value: string): string { return root.serviceCommand(kind, action, value); }
        function network(action: string, id: string): string { return root.networkCommand(action, id); }
        function bluetooth(action: string, id: string): string { return root.bluetoothCommand(action, id); }
        function audio(action: string, input: bool, value: real): string { return root.audioCommand(action, input, value); }
        function media(action: string, player: string, value: real): string { return root.mediaCommand(action, player, value); }
        function notification(action: string, id: string, actionId: string): string { return root.notificationCommand(action, id, actionId); }
        function dock(action: string, id: string, destination: int): string { return root.dockCommand(action, id, destination); }
        function window(action: string, id: string): string { return root.windowCommand(action, id); }
    }
}
