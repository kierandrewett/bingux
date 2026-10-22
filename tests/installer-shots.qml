import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Walks every installer page for installer-shots.py. Each "INSTALLER_SHOT
// <name>" line pauses the script until the harness has grabbed the settled
// frame and rewritten the ack file. Copied into the packed installer tree as
// InstallerShotsDriver.qml; the layout matches InstallerShell.qml.
ShellRoot {
    id: root

    property var pending: []
    property bool paused: false

    function queue(action) {
        pending.push(action);
        drain();
    }

    function drain() {
        while (pending.length > 0 && !paused) {
            const action = pending.shift();
            action();
        }
    }

    property int shotCount: 0

    function shot(name) {
        shotCount += 1;
        console.error("INSTALLER_SHOT " + name + " " + shotCount);
        paused = true;
    }

    // Hold the queue until predicate() is true, then run the action.
    function when(predicate, action) {
        if (predicate()) {
            action();
            return;
        }
        pending.unshift(() => when(predicate, action));
        paused = true;
        waitTimer.restart();
    }

    function go(pageId) {
        wizard.go(pageId);
    }

    function fillAccount() {
        InstallerState.fullName = "Ada Lovelace";
        InstallerState.username = "ada";
        InstallerState.usernameEdited = true;
        InstallerState.password = "correct horse";
        InstallerState.rootPasswordSet = true;
        InstallerState.rootPassword = "root secret";
        InstallerState.autoLogin = true;
        InstallerState.diskPath = "/dev/vda";
    }

    Timer {
        id: waitTimer
        interval: 100
        repeat: false
        onTriggered: { root.paused = false; root.drain(); }
    }

    FileView {
        id: ack
        path: Quickshell.env("INSTALLER_SHOTS_ACK")
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        // Resume only once the harness has acked this shot; spurious change events are ignored.
        onLoaded: {
            if (parseInt(text()) >= root.shotCount) {
                root.paused = false;
                root.drain();
            }
        }
    }

    Component.onCompleted: {
        Quickshell.watchFiles = false;
        SessionContext.enter("kiosk");
        client.probe();
        queue(() => go("welcome"));
        queue(() => root.shot("welcome"));
        queue(() => go("keyboard"));
        queue(() => root.shot("keyboard"));
        queue(() => go("network"));
        queue(() => root.shot("network"));
        queue(() => go("timezone"));
        queue(() => root.shot("timezone"));
        queue(() => go("hostname"));
        queue(() => root.shot("hostname"));
        queue(() => root.when(() => InstallerState.probeState === "ready", () => go("disk")));
        queue(() => root.shot("disk"));
        queue(() => { InstallerState.diskPath = "/dev/vda"; InstallerState.filesystem = "xfs"; InstallerState.swap = "zram"; go("storage"); });
        queue(() => root.shot("storage"));
        queue(() => { root.fillAccount(); go("account"); });
        queue(() => root.shot("account"));
        queue(() => go("summary"));
        queue(() => root.shot("summary"));
        queue(() => {
            InstallerState.installState = "running";
            InstallerState.phase = "copy";
            InstallerState.phaseMessage = "Copying the system";
            InstallerState.percent = 40;
            InstallerState.logLines = ["validate: ok", "preflight: ok", "partition: /dev/vda", "mount: /mnt", "copy: 412 of 1024 packages"];
            go("progress");
        });
        queue(() => root.shot("progress"));
        queue(() => {
            InstallerState.installState = "done";
            InstallerState.percent = 100;
            InstallerState.result = { disk: "/dev/vda", firmware: "uefi", filesystem: "ext4" };
            wizard.replace("finished");
        });
        queue(() => root.shot("finished"));
        queue(() => {
            InstallerState.installState = "failed";
            InstallerState.errorCode = "storage_failed";
            InstallerState.errorMessage = "The partition table could not be written to /dev/vdb.";
            InstallerState.logLines = ["partition: /dev/vdb", "mount: /mnt", "storage_failed: RuntimeError: fake partition table write failed"];
            wizard.replace("error");
        });
        queue(() => root.shot("error"));
        queue(() => {
            console.error("INSTALLER_SHOTS_DONE");
            Qt.quit();
        });
    }

    InstallerClient {
        id: client
    }

    Metrics {
        id: sessionMetrics
    }

    PanelWindow {
        id: window
        color: "transparent"
        screen: Quickshell.screens[0]
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "bingux-installer"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        InstallerWizard {
            id: wizard
            anchors.fill: parent
            metrics: sessionMetrics
            client: client
            hostWindow: window
            wallpaper: Quickshell.env("INSTALLER_SHOTS_WALLPAPER")
            interactive: true
        }
    }
}
