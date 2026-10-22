import QtQuick
import Quickshell

// Drives InstallerClient against the fake daemon in tests/installer-ui.py.
// Copied into the packed installer tree as InstallerUiDriver.qml and run with
// bingux-shell -p. Each passed stage prints one INSTALLER_UI_* marker; any
// failed check prints INSTALLER_UI_FAILED and exits 1. Mode "unreachable"
// points at a socket with no daemon and expects the probe to fail.
ShellRoot {
    id: root

    readonly property string mode: Quickshell.env("INSTALLER_UI_MODE") || "flow"
    // 0: probe, 1: successful install, 2: install that fails at storage.
    property int stage: 0
    property var phases: []

    function fail(message) {
        console.error("INSTALLER_UI_FAILED: " + message);
        Qt.exit(1);
    }

    function report(marker) {
        console.warn(marker);
    }

    function finish() {
        console.warn("INSTALLER_UI_PASSED");
        Qt.exit(0);
    }

    function startInstall(disk) {
        phases = [];
        InstallerState.diskPath = disk;
        client.install(InstallerState.planObject());
    }

    function onProbe(state) {
        if (mode === "unreachable") {
            if (state !== "failed")
                return;
            if (InstallerState.probeError === "")
                fail("probe failed without a message");
            report("INSTALLER_UI_UNREACHABLE_OK");
            finish();
            return;
        }
        if (state === "failed")
            fail("probe failed: " + InstallerState.probeError);
        if (state !== "ready" || stage !== 0)
            return;
        if (InstallerState.firmware !== "uefi")
            fail("firmware is " + InstallerState.firmware);
        if (InstallerState.disks.length !== 2 || InstallerState.disks[0].path !== "/dev/vda")
            fail("disks are " + JSON.stringify(InstallerState.disks));
        report("INSTALLER_UI_PROBE_OK");
        InstallerState.fullName = "Ada Lovelace";
        InstallerState.username = "ada";
        InstallerState.usernameEdited = true;
        InstallerState.password = "correct horse";
        stage = 1;
        startInstall("/dev/vda");
    }

    function onInstalled(data) {
        if (stage !== 1)
            fail("unexpected success at stage " + stage);
        if (InstallerState.installState !== "done")
            fail("installState is " + InstallerState.installState);
        if (data.disk !== "/dev/vda" || InstallerState.percent !== 100)
            fail("result " + JSON.stringify(data) + " percent " + InstallerState.percent);
        const want = ["validate", "preflight", "partition", "mount", "copy", "strip", "configure", "bootloader", "finish"];
        if (JSON.stringify(phases) !== JSON.stringify(want))
            fail("phases " + JSON.stringify(phases));
        report("INSTALLER_UI_INSTALL_OK");
        stage = 2;
        startInstall("/dev/vdb");
    }

    function onFailed(code, message) {
        if (stage !== 2)
            fail("unexpected failure " + code + " at stage " + stage);
        if (code !== "storage_failed" || InstallerState.errorCode !== code)
            fail("failure code " + code + ", state " + InstallerState.errorCode);
        if (InstallerState.installState !== "failed" || message === "")
            fail("installState " + InstallerState.installState);
        report("INSTALLER_UI_FAILURE_OK");
        if (mode === "flow")
            finish();
    }

    Component.onCompleted: client.probe()

    Connections {
        target: InstallerState
        function onProbeStateChanged() {
            root.onProbe(InstallerState.probeState);
        }
        function onPhaseChanged() {
            if (InstallerState.phase !== "")
                root.phases = root.phases.concat([InstallerState.phase]);
        }
    }

    InstallerClient {
        id: client
        onInstallFinished: data => root.onInstalled(data)
        onInstallFailed: (code, message) => root.onFailed(code, message)
    }
}
