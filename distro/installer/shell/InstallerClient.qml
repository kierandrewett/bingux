import QtQuick
import Quickshell
import Quickshell.Io

// NDJSON client for bingux-installer-daemon (distro/installer/README.md,
// Protocol). One Socket per request: the protocol sends one request line,
// then zero or more events and exactly one terminal line. cancel() uses its
// own connection, as the protocol requires.
QtObject {
    id: root

    readonly property string socketPath: Quickshell.env("BINGUX_INSTALLER_SOCKET") || "/run/bingux-installer.sock"
    readonly property string nmcliPath: Quickshell.env("BINGUX_INSTALLER_NMCLI") || "nmcli"

    // Emitted once per install, after InstallerState has been updated.
    signal installFinished(var data)
    signal installFailed(string code, string message)

    property Component requestComponent: Component {
        Socket {
            id: connection
            property string payload: ""
            // Called with (line object) for every line; terminal lines end the request.
            property var onLine: null
            // Called when the connection ends without a terminal line.
            property var onLost: null
            property bool finished: false

            path: root.socketPath
            onConnectedChanged: {
                if (connected) {
                    write(payload + "\n");
                    flush();
                } else if (!finished) {
                    finished = true;
                    if (onLost)
                        onLost();
                    connection.destroy();
                }
            }
            onError: function (_error) {
                if (finished)
                    return;
                finished = true;
                if (onLost)
                    onLost();
                connection.destroy();
            }
            parser: SplitParser {
                onRead: function (line) {
                    if (connection.finished)
                        return;
                    let message;
                    try {
                        message = JSON.parse(line);
                    } catch (error) {
                        return;
                    }
                    const terminal = message.event === "result" || message.event === "error";
                    if (terminal)
                        connection.finished = true;
                    if (connection.onLine)
                        connection.onLine(message);
                    if (terminal)
                        connection.destroy();
                }
            }
            Component.onCompleted: connected = true
        }
    }

    function send(payload, onLine, onLost) {
        return requestComponent.createObject(root, {payload: JSON.stringify(payload), onLine: onLine, onLost: onLost});
    }

    // Probes firmware, disks and tools. Fills InstallerState.
    function probe() {
        InstallerState.probeState = "loading";
        InstallerState.probeError = "";
        send({op: "probe"}, function (message) {
            if (message.event === "result") {
                const data = message.data || {};
                InstallerState.firmware = data.firmware || "";
                InstallerState.liveDisk = data.live_disk || "";
                InstallerState.disks = data.disks || [];
                InstallerState.toolsMissing = data.tools_missing || [];
                InstallerState.installerVersion = data.installer_version || "";
                InstallerState.probeState = "ready";
                if (InstallerState.diskPath !== "" && !InstallerState.selectedDisk)
                    InstallerState.diskPath = "";
            } else if (message.event === "error") {
                InstallerState.probeError = message.message || message.code || "probe failed";
                InstallerState.probeState = "failed";
            }
        }, function () {
            InstallerState.probeError = "The installer service is not reachable.";
            InstallerState.probeState = "failed";
        });
    }

    // Starts the install. The plan is built by InstallerState.planObject()
    // unless the caller passes one.
    function install(plan) {
        InstallerState.resetInstall();
        InstallerState.installState = "running";
        InstallerState.appendLog("Installing to " + plan.disk + " with " + plan.filesystem + ".");
        let finishedHere = false;
        function fail(code, message) {
            if (finishedHere)
                return;
            finishedHere = true;
            InstallerState.installState = "failed";
            InstallerState.errorCode = code;
            InstallerState.errorMessage = message;
            InstallerState.appendLog("Failed (" + code + "): " + message);
            root.installFailed(code, message);
        }
        send({op: "install", plan: plan}, function (message) {
            if (message.event === "progress") {
                InstallerState.phase = message.phase || "";
                InstallerState.percent = (message.percent === null || message.percent === undefined) ? -1 : message.percent;
                InstallerState.phaseMessage = message.message || "";
                if (message.message)
                    InstallerState.appendLog("[" + message.phase + "] " + message.message);
            } else if (message.event === "log") {
                InstallerState.appendLog(message.message || "");
            } else if (message.event === "result") {
                if (finishedHere)
                    return;
                finishedHere = true;
                InstallerState.result = message.data || {};
                InstallerState.percent = 100;
                InstallerState.installState = "done";
                InstallerState.appendLog("Installation finished.");
                root.installFinished(InstallerState.result);
            } else if (message.event === "error") {
                fail(message.code || "internal", message.message || "");
            }
        }, function () {
            fail("unavailable", "The installer service stopped before it finished.");
        });
    }

    function cancel() {
        send({op: "cancel"}, function (message) {
            if (message.event === "error")
                InstallerState.appendLog("Cancel: " + (message.message || message.code));
        }, function () {
            InstallerState.appendLog("Cancel: the installer service is not reachable.");
        });
    }
}
