pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "BsmSession.js" as Session

// Client for bsm's session and power actions, used only in integrated mode
// (BsmMode.integrated). bsm decides, authorises and animates every lock, log
// out, user switch, suspend, restart and power off; this shell asks with
// `bsmctl ACTION --json` and then follows the operation id in the event stream
// of one long-lived `bsmctl monitor --json`. An accepted request is not a
// success: the outcome is whatever the monitor reports. Nothing here draws a
// transition. The pure parts live in BsmSession.js.
QtObject {
    id: root

    // The desktop shell turns this on at start-up (BsmDesktop.qml) so the
    // monitor is attached before any request can be made.
    property bool monitoring: false

    property var tracker: Session.newTracker()
    readonly property var operation: tracker.current
    readonly property bool busy: Session.isActive(tracker)
    readonly property string status: operation ? operation.status : "idle"
    readonly property string phaseLabel: operation ? Session.phaseLabel(operation.phase) : ""
    readonly property string failureText: operation && (operation.status === "failed" || operation.status === "cancelled") ? Session.describeFailure(operation.code, operation.detail) : ""

    // Emitted once per operation when it reaches Completed, Failed or Cancelled.
    signal finished(var operation)

    property bool notifyOnFailure: false

    function setTracker(next) {
        const before = tracker.current ? tracker.current.status : "idle";
        tracker = next;
        const after = next.current ? next.current.status : "idle";
        if (before === after || !next.current)
            return;
        if (["completed", "failed", "cancelled"].includes(after)) {
            if (after === "failed" && notifyOnFailure)
                ShellNotifications.send(Session.titleFor(next.current.action), failureText);
            notifyOnFailure = false;
            finished(next.current);
        }
    }

    // Ask bsm to perform ACTION (lock, logout, switch-user, suspend, reboot,
    // poweroff). Returns false when the request was not started: not in
    // integrated mode, an unknown action or another request still open.
    function request(action, options) {
        if (!BsmMode.integrated || busy)
            return false;
        const command = Session.requestCommand(action);
        if (!command)
            return false;
        notifyOnFailure = !!(options && options.notify);
        setTracker(Session.begin(tracker, action));
        requestProcess.started = false;
        requestProcess.command = command;
        requestProcess.running = true;
        return true;
    }

    // Forget a finished operation so the UI returns to its idle state.
    function dismiss() {
        setTracker(Session.clearFinished(tracker));
    }

    property Process requestProcess: Process {
        property bool started: false
        stdout: StdioCollector {
            id: requestOutput
        }
        stderr: StdioCollector {
            id: requestErrors
        }
        onStarted: started = true
        onExited: code => root.setTracker(Session.answered(root.tracker, Session.parseRequest(code, requestOutput.text, requestErrors.text)))
        // A binary that cannot be started never exits: it only stops running.
        onRunningChanged: if (!running && !started && root.tracker.current && root.tracker.current.status === "requesting")
            root.setTracker(Session.answered(root.tracker, {
                ok: false,
                code: "unavailable",
                detail: ""
            }))
    }

    readonly property bool monitorWanted: BsmMode.integrated && monitoring
    onMonitorWantedChanged: if (monitorWanted && !monitorProcess.running)
        startMonitor()

    function startMonitor() {
        monitorProcess.started = false;
        monitorProcess.running = true;
    }

    property int monitorBackoff: 1000

    property Process monitorProcess: Process {
        property bool started: false
        command: Session.monitorCommand()
        stdout: SplitParser {
            onRead: line => {
                const event = Session.parseMonitorEvent(line);
                if (event === null)
                    return;
                root.monitorBackoff = 1000;
                root.setTracker(Session.observe(root.tracker, event));
            }
        }
        onStarted: started = true
        onExited: root.monitorEnded()
        onRunningChanged: if (!running && !started)
            root.monitorEnded()
    }

    // The monitor is the only evidence of an outcome, so an operation that was
    // open when it ended fails visibly; the monitor then restarts with a
    // growing delay (no tight loop when bsmctl is missing or bsm is down).
    function monitorEnded() {
        setTracker(Session.monitorLost(tracker));
        if (monitorWanted) {
            monitorRetry.interval = monitorBackoff;
            monitorBackoff = Math.min(monitorBackoff * 2, 30000);
            monitorRetry.restart();
        }
    }

    property Timer monitorRetry: Timer {
        onTriggered: if (root.monitorWanted && !monitorProcess.running)
            root.startMonitor()
    }
}
