import QtQuick
import Quickshell.Io
import "InstallerSchema.js" as Schema

// The install failed. Back returns to the summary; Retry starts the install
// again with the same plan. The hint says whether the disk was touched.
InstallerPage {
    id: page

    title: Schema.errorTitle(InstallerState.errorCode)
    allowBack: true
    primaryText: "Retry"
    bodyHeight: 520
    secondaryText: "Save log"
    secondaryVisible: true

    // InstallerClient, which starts the retry.
    property var client: null
    property bool showLog: false
    property string savedPath: ""

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

    primaryEnabled: InstallerState.installState !== "running"

    function primary() {
        page.wizard.replace("progress");
        page.client.install(InstallerState.planObject());
    }

    function secondary() {
        const stamp = Schema.nowStamp();
        page.savedPath = "/tmp/bingux-installer-" + stamp + ".log";
        logFile.path = page.savedPath;
        logFile.setText(InstallerState.logLines.join("\n") + "\n");
    }

    FileView {
        id: logFile
        blockWrites: false
        atomicWrites: false
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            tone: "danger"
            text: InstallerState.errorMessage !== "" ? InstallerState.errorMessage : "The installation did not finish."
        }
        InstallerText {
            tone: "text"
            text: page.hint
        }
        InstallerText {
            visible: page.savedPath !== ""
            tone: "success"
            text: "Log saved to " + page.savedPath
        }
        ActionButton {
            flat: true
            text: page.showLog ? "Hide log" : "View log"
            onClicked: page.showLog = !page.showLog
        }
        InstallerLog {
            visible: page.showLog
            width: parent.width
            lineHeight: 220
        }
    }
}
