import QtQuick
import Quickshell.Io
import "InstallerSchema.js" as Schema

// The install failed. The primary action depends on why: a failure that
// trying again can fix starts the install again with the same plan; one that
// the plan caused (a disk too small, a disk that holds the installer) leads
// back to the choice that must change. The notice says whether the disk was
// touched; View log and Save log are large buttons in the body.
InstallerPage {
    id: page

    title: Schema.errorTitle(InstallerState.errorCode)
    lead: beforeCommit.indexOf(InstallerState.errorCode) >= 0
        ? "The installation did not start. Read the notice, then keep the log if you need it."
        : "The installation stopped. Read the notice, then try again or keep the log."
    fullBody: true
    allowBack: true
    primaryText: action === "disk" ? "Choose another disk" : action === "settings" ? "Review settings" : "Try again"
    primaryVisible: action !== "none"
    bodyHeight: 600
    secondaryVisible: false

    // InstallerClient, which starts the retry.
    property var client: null
    property bool showLog: false
    property string savedPath: ""
    readonly property real us: InstallerState.uiScale

    // Codes the daemon returns before the first destructive write.
    readonly property var beforeCommit: ["bad_request", "not_permitted", "busy", "invalid_plan", "unsupported",
        "disk_not_found", "disk_is_live_medium", "disk_too_small", "other_os_present", "missing_tool",
        "cancelled", "no_install", "cancel_refused_after_commit"]
    // Codes that happen after the commit point, or can happen after it.
    readonly property var afterCommit: ["storage_failed", "config_failed", "bootloader_failed", "payload_failed"]

    // What the primary button does: "retry" runs the same plan again, "disk"
    // and "settings" go back to the choice the failure points at, and "none"
    // offers nothing because trying again cannot change the result.
    readonly property string action: {
        const code = InstallerState.errorCode;
        if (["disk_too_small", "disk_not_found", "disk_is_live_medium", "other_os_present"].indexOf(code) >= 0)
            return "disk";
        if (["invalid_plan", "unsupported"].indexOf(code) >= 0)
            return "settings";
        if (code === "missing_tool")
            return "none";
        return "retry";
    }

    readonly property string hint: {
        const code = InstallerState.errorCode;
        if (beforeCommit.indexOf(code) >= 0)
            return "Nothing on the disk was changed.";
        if (afterCommit.indexOf(code) >= 0)
            return "The disk may be partly written. Run the install again to start over.";
        return "If the install reached the partition step, the disk may be partly written. Check the log before you trust the disk.";
    }
    readonly property string diskTitle: "Disk state"
    readonly property string diskTone: beforeCommit.indexOf(InstallerState.errorCode) >= 0 ? "info" : "danger"

    primaryEnabled: InstallerState.installState !== "running"

    function primary() {
        if (page.action === "disk") {
            page.wizard.go("disk");
        } else if (page.action === "settings") {
            page.wizard.go("summary");
        } else if (page.action === "retry") {
            page.wizard.replace("progress");
            page.client.install(InstallerState.planObject());
        }
    }

    function saveLog() {
        const stamp = Schema.nowStamp();
        page.savedPath = "/tmp/bingux-installer-" + stamp + ".log";
        logFile.path = page.savedPath;
        logFile.setText(InstallerState.logText() + "\n");
    }

    FileView {
        id: logFile
        blockWrites: false
        atomicWrites: false
    }

    InstallerScroll {
        spacing: Math.round(18 * page.us)

        InstallerText {
            tone: "danger"
            text: InstallerState.errorMessage !== "" ? InstallerState.errorMessage : "The installation did not finish."
        }
        InstallerNotice {
            tone: page.diskTone
            title: page.diskTitle
            body: page.hint
        }

        Row {
            spacing: Theme.gap * 2

            ActionButton {
                width: Math.round(220 * page.us)
                height: Math.round(60 * page.us)
                text: page.showLog ? "Hide log" : "View log"
                onClicked: page.showLog = !page.showLog
            }
            ActionButton {
                width: Math.round(220 * page.us)
                height: Math.round(60 * page.us)
                text: "Save log"
                onClicked: page.saveLog()
            }
        }
        InstallerText {
            visible: page.savedPath !== ""
            tone: "success"
            text: "Log saved to " + page.savedPath
        }
        InstallerLog {
            visible: page.showLog
            width: parent.width
            lineHeight: Math.round(260 * page.us)
        }
    }
}
