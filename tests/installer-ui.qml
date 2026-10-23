import QtQuick
import Quickshell

// Drives InstallerClient against the fake daemon in tests/installer-ui.py.
// Copied into the packed installer tree as InstallerUiDriver.qml and run with
// bingux-shell -p. Each passed stage prints one INSTALLER_UI_* marker; any
// failed check prints INSTALLER_UI_FAILED and exits 1. Mode "unreachable"
// points at a socket with no daemon and expects the probe to fail. Mode "log"
// appends 5000 lines to the install log without a daemon and checks the cap.
// Mode "manual" builds a partition list on /dev/vdc (reuse the ESP, format root, new swap) and installs it.
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
        if (InstallerState.disks.length !== 3 || InstallerState.disks[0].path !== "/dev/vda" || InstallerState.disks[2].path !== "/dev/vdc")
            fail("disks are " + JSON.stringify(InstallerState.disks));
        if (!InstallerState.offers("filesystems", "xfs") || !InstallerState.offers("swap", "zram"))
            fail("probe capabilities are " + JSON.stringify(InstallerState.capabilities));
        report("INSTALLER_UI_PROBE_OK");
        setAccount();
        stage = 1;
        if (mode === "manual") {
            startManual();
            return;
        }
        InstallerState.filesystem = "xfs";
        InstallerState.swap = "zram";
        InstallerState.ntp = false;
        startInstall("/dev/vda");
    }

    function setAccount() {
        InstallerState.fullName = "Ada Lovelace";
        InstallerState.username = "ada";
        InstallerState.usernameEdited = true;
        InstallerState.password = "correct horse";
        InstallerState.rootPasswordSet = true;
        InstallerState.rootPassword = "root secret";
        InstallerState.autoLogin = true;
    }

    // Manual mode: reuse the ESP, format root, and create a 2 GiB swap partition on /dev/vdc.
    function startManual() {
        InstallerState.diskPath = "/dev/vdc";
        InstallerState.storageMode = "manual";
        InstallerState.setSwap("partition");
        InstallerState.encryption = false;
        const list = [
            {role: "esp", action: "reuse", device: "/dev/vdc1", sizeText: "", typed: false, fs: "ext4"},
            {role: "root", action: "format", device: "/dev/vdc2", sizeText: "", typed: false, fs: "ext4"},
            {role: "swap", action: "create", device: "/dev/vdc3", sizeText: "2", typed: true, fs: "ext4"}
        ];
        InstallerState.manualEntries = list;
        const problems = InstallerState.manualProblems();
        if (problems.length !== 0)
            fail("manual list has problems: " + problems.join("; "));
        // A separate home partition cannot be encrypted yet: the schema mirror must say so.
        InstallerState.manualEntries = list.concat([{role: "home", action: "create", device: "/dev/vdc4", sizeText: "1", typed: true, fs: "ext4"}]);
        InstallerState.encryption = true;
        if (InstallerState.manualProblems().indexOf("A separate home partition cannot be encrypted yet.") < 0)
            fail("encrypted home is not refused: " + JSON.stringify(InstallerState.manualProblems()));
        InstallerState.encryption = false;
        InstallerState.manualEntries = list;
        if (InstallerState.manualProblems().length !== 0)
            fail("the list does not validate after undoing the home partition");
        report("INSTALLER_UI_MANUAL_CHECK_OK");
        const plan = InstallerState.planObject();
        const want = [
            {device: "/dev/vdc1", action: "reuse", role: "esp"},
            {device: "/dev/vdc2", action: "format", role: "root"},
            {device: "/dev/vdc3", action: "create", role: "swap", size_bytes: 2 * 1073741824}
        ];
        if (plan.storage.mode !== "manual" || plan.confirm_erase !== "/dev/vdc")
            fail("plan storage is " + JSON.stringify(plan.storage) + ", confirm_erase " + plan.confirm_erase);
        if (JSON.stringify(plan.storage.partitions) !== JSON.stringify(want))
            fail("plan partitions are " + JSON.stringify(plan.storage.partitions));
        startInstall("/dev/vdc");
    }

    function onInstalled(data) {
        if (stage !== 1)
            fail("unexpected success at stage " + stage);
        if (InstallerState.installState !== "done")
            fail("installState is " + InstallerState.installState);
        const disk = mode === "manual" ? "/dev/vdc" : "/dev/vda";
        if (data.disk !== disk || InstallerState.percent !== 100)
            fail("result " + JSON.stringify(data) + " percent " + InstallerState.percent);
        const want = ["validate", "preflight", "partition", "mount", "copy", "strip", "configure", "bootloader", "finish"];
        if (JSON.stringify(phases) !== JSON.stringify(want))
            fail("phases " + JSON.stringify(phases));
        if (InstallerState.logModel.count === 0)
            fail("install wrote no log lines");
        report("INSTALLER_UI_INSTALL_OK");
        if (mode === "manual") {
            report("INSTALLER_UI_MANUAL_OK");
            finish();
            return;
        }
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

    function runLogCheck() {
        const total = 5000;
        for (let i = 0; i < total; i++)
            InstallerState.appendLog("line " + i);
        const model = InstallerState.logModel;
        const count = model.count;
        if (count > InstallerState.logLimit)
            fail("log holds " + count + " lines, limit " + InstallerState.logLimit);
        if (count < InstallerState.logKeep)
            fail("log holds " + count + " lines, fewer than the kept " + InstallerState.logKeep);
        if (model.get(count - 1).line !== "line " + (total - 1))
            fail("newest line is " + model.get(count - 1).line);
        for (let i = 1; i < count; i++) {
            const want = "line " + (total - count + i);
            if (model.get(i).line !== want)
                fail("line " + i + " is " + model.get(i).line + ", want " + want);
        }
        const text = InstallerState.logText();
        if (text.split("\n").length !== count || text.indexOf("line " + (total - 1)) < 0)
            fail("logText does not hold the retained lines");
        InstallerState.resetInstall();
        if (model.count !== 0)
            fail("resetInstall left " + model.count + " lines");
        report("INSTALLER_UI_LOG_OK");
        finish();
    }

    Component.onCompleted: mode === "log" ? runLogCheck() : client.probe()

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
