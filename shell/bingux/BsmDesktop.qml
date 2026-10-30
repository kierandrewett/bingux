import QtQuick
import Quickshell

// The desktop shell's part of bsm integration, loaded by shell.qml only when
// BsmMode.integrated. It watches the top bar and the dock for readiness
// (BsmReadiness.qml) and starts the session-action monitor (BsmSession.qml).
// It never reads Startup.settled and changes none of the shell's own state.
//
// shell.qml sets topBar, dock and settings, then calls start().
QtObject {
    id: root

    property var topBar: null
    property var dock: null
    property var settings: null
    property bool started: false

    function start() {
        if (started)
            return;
        started = true;
        BsmSession.monitoring = true;
        barProbe.attach();
        dockProbe.attach();
    }

    property BsmSurfaceProbe barProbe: BsmSurfaceProbe {
        window: root.topBar
        role: "bar"
        component: "desktop"
    }

    property BsmSurfaceProbe dockProbe: BsmSurfaceProbe {
        window: root.dock
        role: "dock"
        component: "desktop"
    }

    // The dock is required only when the configuration enables it; the
    // preferences must be read before the set of required surfaces is known.
    property Binding loaded: Binding {
        target: BsmReadiness
        property: "configLoaded"
        value: BinguxPreferences.loaded
        when: root.started
    }

    property Binding dockEnabled: Binding {
        target: BsmReadiness
        property: "dockEnabled"
        value: !!root.settings && root.settings.dockEnabled === true
        when: root.started
    }
}
