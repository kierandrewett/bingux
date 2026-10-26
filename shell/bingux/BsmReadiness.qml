pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "BsmReadiness.js" as Protocol

// Tells bsm's bridge (bsm-host) what this Bingux process has really put on
// screen, as JSON lines on $XDG_RUNTIME_DIR/bsm/ready.sock. Used only in
// integrated mode (BsmMode.integrated); every instance of BsmSurfaceProbe
// registers here and nothing else creates this object.
//
// Each Quickshell process is one `component` (wallpaper or desktop) and sends
// launched, one surface line per required window, then ready once every
// required surface of the component has rendered a frame. Startup.settled is
// never read: it can be a timer. docs/bsm/shell-integration.md has the wire
// format.
QtObject {
    id: root

    // Set by the first probe; one process serves one component.
    property string component: ""
    // Dock preference and whether the preferences have been read. The desktop
    // shell updates it (BsmDesktop.qml); the wallpaper has no configuration.
    property bool configLoaded: false
    property bool dockEnabled: false

    property var probes: []
    property int revision: 0

    readonly property var evaluation: {
        revision;
        const names = Quickshell.screens.map(screen => screen.name);
        return Protocol.evaluate({
            component: component,
            config: {
                loaded: component === "wallpaper" || configLoaded,
                dock: dockEnabled
            },
            screens: names,
            windows: probes.map(probe => ({
                        role: probe.role,
                        output: probe.output,
                        rendered: probe.rendered
                    }))
        });
    }
    onEvaluationChanged: flush()

    // The lines written on the current connection, key to text.
    property var sent: ({})
    property bool warned: false
    property int retryDelay: 500

    function changed() {
        revision++;
    }

    function register(probe) {
        if (!Protocol.validComponent(probe.component)) {
            console.warn("[bsm] readiness probe with unknown component '" + probe.component + "' ignored");
            return;
        }
        if (component === "")
            component = probe.component;
        if (probes.includes(probe))
            return;
        probes = probes.concat([probe]);
        revision++;
        connect();
    }

    function unregister(probe) {
        if (!probes.includes(probe))
            return;
        probes = probes.filter(item => item !== probe);
        revision++;
    }

    // Nothing is written until the socket is connected; a new connection starts
    // from an empty `sent`, so a restarted bridge or shell reports everything
    // it knows and the lines stay idempotent for the bridge.
    function flush() {
        if (!socket.connected || component === "")
            return;
        const result = Protocol.diff(BsmMode.generation, component, evaluation, sent);
        sent = result.sent;
        for (const line of result.lines)
            socket.write(line + "\n");
        if (result.lines.length > 0)
            socket.flush();
    }

    function connect() {
        if (socket.connected || retry.running)
            return;
        if (!BsmMode.generationValid) {
            if (!warned)
                console.warn("[bsm] BSM_GENERATION is missing or not a decimal number; readiness is not reported");
            warned = true;
            return;
        }
        socket.path = BsmMode.readySocketPath;
        socket.connected = true;
    }

    function lost(reason) {
        if (retry.running)
            return;
        sent = {};
        if (!warned)
            console.warn("[bsm] cannot reach " + BsmMode.readySocketPath + " (" + reason + "); retrying quietly");
        warned = true;
        retry.interval = retryDelay;
        retryDelay = Math.min(retryDelay * 2, 15000);
        retry.restart();
    }

    property Timer retry: Timer {
        onTriggered: {
            socket.connected = false;
            root.connect();
        }
    }

    property Socket socket: Socket {
        onConnectionStateChanged: {
            if (connected) {
                root.warned = false;
                root.retryDelay = 500;
                root.sent = {};
                root.flush();
            } else {
                root.lost("disconnected");
            }
        }
        onError: error => root.lost("socket error " + error)
    }
}
