import QtQuick
import "InstallerSchema.js" as Schema

// Disk choice from the probe. The service erases a whole disk; the other ways
// of using a disk are listed but disabled until the protocol carries them.
InstallerPage {
    id: page

    title: "Disk"
    bodyHeight: 470

    onActiveChanged: {
        if (active && client)
            client.probe();
    }

    // InstallerClient, used to probe again.
    property var client: null

    readonly property var disk: InstallerState.selectedDisk
    readonly property string diskSummary: disk ? disk.path + " (" + Schema.formatBytes(disk.size_bytes) + ", " + (disk.model || "unknown model") + ")" : ""
    readonly property var otherOs: disk && disk.other_os ? disk.other_os : []
    readonly property bool otherOsOnDisk: otherOs.length > 0

    primaryEnabled: disk !== null && diskProblem(disk) === "" && (!otherOsOnDisk || InstallerState.otherOsAllow)

    function diskProblem(candidate) {
        if (candidate.is_live_medium)
            return "Holds the installer; it cannot be used.";
        if (candidate.size_bytes < Schema.MIN_DISK_BYTES)
            return "Too small; Bingux needs at least 8 GiB.";
        return "";
    }

    function primary() {
        page.wizard.go("storage");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            visible: InstallerState.probeState === "loading"
            text: "Looking for disks..."
        }
        InstallerText {
            visible: InstallerState.probeState === "failed"
            tone: "danger"
            text: InstallerState.probeError
        }
        ActionButton {
            visible: InstallerState.probeState === "failed"
            flat: true
            text: "Try again"
            onClicked: page.client.probe()
        }
        InstallerText {
            visible: InstallerState.probeState === "ready" && InstallerState.disks.length === 0
            tone: "danger"
            text: "No disks were found on this computer."
        }
        Repeater {
            model: InstallerState.disks
            SessionRow {
                required property var modelData
                readonly property string problem: page.diskProblem(modelData)
                width: parent.width
                enabled: problem === ""
                text: modelData.path + "   " + Schema.formatBytes(modelData.size_bytes) + "   " + (modelData.model || "unknown model")
                    + (problem !== "" ? "   (" + problem + ")" : "")
                selected: InstallerState.diskPath === modelData.path
                ticked: selected
                onClicked: InstallerState.diskPath = modelData.path
            }
        }

        InstallerText {
            visible: page.disk !== null
            tone: "danger"
            text: "Everything on " + page.diskSummary + " will be erased when you install. Bingux cannot undo this."
        }
        InstallerText {
            visible: page.otherOsOnDisk
            tone: "warning"
            text: "This disk holds " + page.otherOs.join(", ") + ". Erasing it removes that system too."
        }
        InstallerSwitchRow {
            visible: page.otherOsOnDisk
            label: "Erase the other system too"
            checked: InstallerState.otherOsAllow
            onToggled: value => InstallerState.otherOsAllow = value
        }

        InstallerText {
            tone: "text"
            text: "Other ways to use a disk"
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Erase the whole disk (selected)"
            selected: true
            ticked: true
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Use free space (not yet available)"
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Install alongside another system (not yet available)"
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Resize an existing partition (not yet available)"
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Partition by hand (not yet available)"
        }
    }
}
