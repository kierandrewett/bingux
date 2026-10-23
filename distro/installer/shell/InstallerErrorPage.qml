import QtQuick
import Quickshell.Io
import "InstallerSchema.js" as Schema

// The install failed. Retry starts the install again with the same plan. The
// notice says whether the disk was touched; View log and Save log are large
// buttons in the body.
InstallerPage {
    id: page

    title: Schema.errorTitle(InstallerState.errorCode)
    lead: "The installation stopped. Read the notice, then retry or keep the log."
    fullBody: true
    allowBack: true
    primaryText: "Retry"
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
        page.wizard.replace("progress");
        page.client.install(InstallerState.planObject());
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
