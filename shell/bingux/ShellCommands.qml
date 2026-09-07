import QtQuick
import Quickshell
import Quickshell.Io

// Public command actions share the same objects and operations as the UI.
Scope {
    id: root
    required property var indicators
    required property var mediaControls
    required property var notificationState
    required property var dockView
    property var windowIds: new Map()
    property int nextWindowId: 0
    readonly property string instance: Date.now().toString(36)
    function ok(value) { return JSON.stringify(value || {ok: true}); }
    function fail(message) { return JSON.stringify({ok: false, error: message}); }
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
        function audio(action: string, input: bool, value: real): string { return root.audioCommand(action, input, value); }
        function media(action: string, player: string, value: real): string { return root.mediaCommand(action, player, value); }
        function notification(action: string, id: string, actionId: string): string { return root.notificationCommand(action, id, actionId); }
        function dock(action: string, id: string, destination: int): string { return root.dockCommand(action, id, destination); }
        function window(action: string, id: string): string { return root.windowCommand(action, id); }
    }
}
