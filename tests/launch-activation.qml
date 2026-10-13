import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Bingux.Wayland

ShellRoot {
    id: root

    readonly property bool offscreenCheck: Quickshell.env("BINGUX_ACTIVATION_OFFSCREEN") === "1"
    // Test-only opt-ins. Zero preserves immediate, single-dispatch behavior.
    readonly property int dispatchDelayMs: delaySetting("BINGUX_ACTIVATION_DELAY_MS")
    readonly property int replayDelayMs: delaySetting("BINGUX_ACTIVATION_REPLAY_MS")
    property int requestSequence: 0
    property int completed: 0
    property bool sourceOpen: true
    property var fixtureEntry: null
    property var fixtureAction: null
    readonly property bool fixtureReady: fixtureEntry !== null && fixtureAction !== null
    readonly property string fixtureDirectory: decodeURIComponent(Qt.resolvedUrl(".").toString().replace(/^file:\/\//, "")).replace(/\/$/, "")

    function delaySetting(name) {
        const value = Quickshell.env(name) || "0";
        if (!/^[0-9]+$/.test(value) || Number(value) > 600000)
            throw new Error(name + " must be an integer from 0 to 600000 milliseconds");
        return Number(value);
    }

    function refreshFixtureEntry() {
        root.fixtureEntry = DesktopEntries.byId("dev.bingux.ActivationProbe");
        root.fixtureAction = root.fixtureEntry?.actions.find(candidate => candidate.id === "proof") || null;
        console.info("BINGUX_ACTIVATION_FIXTURE_READY", JSON.stringify({
            ready: root.fixtureReady,
            desktopId: root.fixtureEntry?.id || "",
            actionId: root.fixtureAction?.id || ""
        }));
    }

    Connections {
        // Instantiate the singleton before a click. Its initial scan publishes
        // asynchronously; byId() alone is not a readiness notification.
        target: DesktopEntries
        function onApplicationsChanged() {
            root.refreshFixtureEntry();
        }
    }

    function launch(source, action, dismiss) {
        const entry = root.fixtureEntry;
        const desktopAction = root.fixtureAction;
        if (!root.fixtureReady) {
            console.warn("Wait for BINGUX_ACTIVATION_FIXTURE_READY before launching; no gesture is queued");
            return;
        }
        const requestId = ++root.requestSequence;
        const requestedAt = Date.now();
        console.info("BINGUX_ACTIVATION_REQUEST", JSON.stringify({
            requestId: requestId,
            delayMs: root.dispatchDelayMs,
            replayMs: root.replayDelayMs,
            action: action ? desktopAction.id : "default",
            label: action ? desktopAction.name : entry.name,
            source: source ? "gesture" : "tokenless",
            dismiss: dismiss
        }));
        const scheduleDispatch = token => {
            const command = ["python3", root.fixtureDirectory + "/launch-application.py"];
            command.push("--host-launch");
            if (action)
                command.push("--action=" + desktopAction.id);
            if (token)
                command.push("--activation-token=" + token);
            command.push("--", entry.id);
            let attempt = 0;
            const dispatch = () => {
                console.info("BINGUX_ACTIVATION_DISPATCH", JSON.stringify({
                    requestId: requestId,
                    attempt: ++attempt,
                    elapsedMs: Date.now() - requestedAt,
                    tokenPresent: !!token
                }));
                const process = launcher.createObject(root, {
                    command: command
                });
                process.running = true;
            };
            // The command (including its opaque token) is captured once. Replay
            // never calls activation.request() or reads another click's state.
            const replayMs = token ? root.replayDelayMs : 0;
            if (root.replayDelayMs > 0 && !token)
                console.warn("BINGUX_ACTIVATION_REPLAY_SKIPPED", requestId, "no token");
            if (root.dispatchDelayMs === 0 && replayMs === 0) {
                dispatch();
                return;
            }
            const timer = pendingDispatch.createObject(root, {
                dispatch: dispatch,
                replayMs: replayMs
            });
            // Measure the initial delay from the ready gesture, not token reply.
            const remainingMs = root.dispatchDelayMs - (Date.now() - requestedAt);
            if (remainingMs > 0) {
                timer.interval = remainingMs;
                timer.start();
            } else {
                timer.deliver();
            }
        };
        if (source) {
            activation.request(source, (token, reason) => {
                console.info("BINGUX_ACTIVATION_REPLY", JSON.stringify({
                    requestId: requestId,
                    tokenPresent: !!token,
                    reason: reason
                }));
                scheduleDispatch(token);
            });
        } else {
            scheduleDispatch("");
        }
        // Match ShellPopup: dismiss input/focus while retaining its surface.
        // No focus-policy change or deferred invisible keyboard grab.
        if (dismiss)
            root.sourceOpen = false;
    }

    Component {
        id: pendingDispatch
        Timer {
            property var dispatch
            property int replayMs: 0
            repeat: false

            // Root ownership survives source dismissal. Each request has its
            // own timer; exiting the fixture destroys all outstanding timers.
            function deliver() {
                dispatch();
                if (replayMs > 0) {
                    interval = replayMs;
                    replayMs = 0;
                    start();
                } else {
                    dispatch = null;
                    destroy();
                }
            }
            onTriggered: deliver()
        }
    }

    Component {
        id: launcher
        Process {
            stderr: SplitParser {
                onRead: data => console.warn("BINGUX_ACTIVATION_HELPER", data)
            }
            onExited: (code, status) => {
                console.info("BINGUX_ACTIVATION_LAUNCH_EXIT", code, status);
                destroy();
            }
        }
    }

    LaunchActivation {
        id: activation
    }

    Component.onCompleted: {
        root.refreshFixtureEntry();
        // GTK 4's GDK constructor stashes and unsets activation variables before
        // the C fixture's main(). Its getenv header observes post-constructor
        // state, not the environment received by exec or GDK's retained token.
        console.info("BINGUX_ACTIVATION_ENV_NOTE", "GTK main() token_present=false does not prove token transport failed; GDK can consume it before main()");
        if (!offscreenCheck)
            return;
        for (let index = 0; index < 2; index++) {
            activation.request(null, (token, reason) => {
                if (token !== "" || reason.length === 0) {
                    console.error("Unexpected tokenless callback:", typeof token, reason);
                    Qt.exit(1);
                }
                root.completed++;
                if (root.completed === 2) {
                    console.info("BINGUX_ACTIVATION_OFFSCREEN_PASS");
                    Qt.callLater(() => Qt.exit(0));
                }
            });
        }
    }

    Timer {
        running: root.offscreenCheck
        interval: 2500
        onTriggered: Qt.exit(1)
    }

    // Native surfaces below are loaded only by the VM fixture. The offscreen
    // runner uses the prefix above: Qt has no layer-shell backend offscreen.
    LazyLoader {
        active: !root.offscreenCheck
        FloatingWindow {
            title: "Bingux activation fixture controls"
            implicitWidth: 540
            implicitHeight: 110
            color: "#252932"

            Row {
                anchors.centerIn: parent
                spacing: 12
                Button {
                    text: "Reopen gesture surface"
                    onClicked: root.sourceOpen = true
                }
                Button {
                    text: "Close fixture"
                    onClicked: Qt.quit()
                }
            }
        }
    }

    LazyLoader {
        active: !root.offscreenCheck
        PanelWindow {
            id: sourceWindow
            visible: true
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.namespace: "bingux-activation-fixture"
            WlrLayershell.keyboardFocus: root.sourceOpen ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }
            mask: Region {
                item: card
            }
            Rectangle {
                id: card
                anchors.centerIn: parent
                width: root.sourceOpen ? 540 : 0
                height: root.sourceOpen ? 250 : 0
                visible: root.sourceOpen
                enabled: root.sourceOpen
                color: "#252932"
                radius: 12

                Column {
                    anchors.centerIn: parent
                    spacing: 8
                    Text {
                        text: root.fixtureReady ? "Real gesture source — strict focus stays enabled" : "Waiting for the fixture desktop entry and proof action..."
                        color: "white"
                    }
                    Row {
                        spacing: 8
                        Button {
                            text: "Default: held"
                            enabled: root.fixtureReady
                            onClicked: root.launch(card, false, false)
                        }
                        Button {
                            text: "Default: dismissed"
                            enabled: root.fixtureReady
                            onClicked: root.launch(card, false, true)
                        }
                    }
                    Row {
                        spacing: 8
                        Button {
                            text: "Action: held"
                            enabled: root.fixtureReady
                            onClicked: root.launch(card, true, false)
                        }
                        Button {
                            text: "Action: dismissed"
                            enabled: root.fixtureReady
                            onClicked: root.launch(card, true, true)
                        }
                    }
                    Row {
                        spacing: 8
                        Button {
                            text: "Tokenless control"
                            enabled: root.fixtureReady
                            onClicked: root.launch(null, true, true)
                        }
                        Button {
                            text: "Release source"
                            onClicked: root.sourceOpen = false
                        }
                    }
                }
            }
        }
    }
}
