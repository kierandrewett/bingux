import QtQuick
import "InstallerSchema.js" as Schema

// Disk and how Bingux uses it. The probe's partitions and capabilities decide
// what is offered: a way of using the disk is enabled only when this installer
// build implements it (capabilities.storage_modes) and the disk can take it.
InstallerPage {
    id: page

    title: "Disk"
    bodyHeight: 600

    onActiveChanged: {
        if (active && client)
            client.probe();
    }

    // InstallerClient, used to probe again.
    property var client: null

    // Room left for Bingux when installing alongside: the partition shrinks by this much.
    readonly property real keepForBingux: 20 * InstallerState.gib

    readonly property var disk: InstallerState.selectedDisk
    readonly property var otherOs: disk && disk.other_os ? disk.other_os : []
    readonly property bool otherOsOnDisk: otherOs.length > 0
    readonly property var partitions: disk && disk.partitions ? disk.partitions : []
    readonly property string mode: InstallerState.storageMode

    readonly property bool freeSpaceOffered: InstallerState.offers("storage_modes", "free_space")
    readonly property bool alongsideOffered: InstallerState.offers("storage_modes", "alongside")
    readonly property bool freeSpaceUsable: disk !== null && disk.free_bytes !== null && disk.free_bytes >= Schema.MIN_DISK_BYTES

    primaryEnabled: disk !== null && diskProblem(disk) === "" && modeProblem() === ""
        && (mode !== "erase" || !otherOsOnDisk || InstallerState.otherOsAllow)

    function diskProblem(candidate) {
        if (candidate.is_live_medium)
            return "Holds the installer; it cannot be used.";
        if (candidate.size_bytes < Schema.MIN_DISK_BYTES)
            return "Too small; Bingux needs at least 8 GiB.";
        return "";
    }

    // Why the chosen mode cannot be installed yet; "" when it can.
    function modeProblem() {
        if (mode === "free_space" && !freeSpaceUsable)
            return "no unallocated space";
        if (mode === "alongside" && InstallerState.shrinkPartition === "")
            return "no partition chosen";
        return "";
    }

    function shrinkProblem(part) {
        if (part.shrinkable !== true)
            return "cannot be shrunk";
        if (part.size_bytes === null || part.min_bytes === null)
            return "size unknown";
        if (part.size_bytes - keepForBingux < part.min_bytes)
            return "not enough room to free " + Schema.formatBytes(keepForBingux);
        return "";
    }

    function chooseDisk(path) {
        InstallerState.diskPath = path;
        InstallerState.storageMode = "erase";
        InstallerState.shrinkPartition = "";
    }

    function choosePartition(part) {
        InstallerState.storageMode = "alongside";
        InstallerState.shrinkPartition = part.path;
        InstallerState.shrinkBytes = part.size_bytes - keepForBingux;
    }

    function partitionLine(part) {
        let text = part.path + "   " + (part.fstype || "no filesystem");
        if (part.label)
            text += "   " + part.label;
        text += "   " + Schema.formatBytes(part.size_bytes);
        if (part.used_bytes !== null && part.used_bytes !== undefined)
            text += "   " + Schema.formatBytes(part.used_bytes) + " used";
        if (part.os)
            text += "   " + part.os;
        return text;
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
                onClicked: page.chooseDisk(modelData.path)
            }
        }

        InstallerText {
            visible: page.disk !== null
            tone: "text"
            text: page.disk
                ? (page.disk.partition_table ? page.disk.partition_table.toUpperCase() : "No partition table")
                    + "   free space " + (page.disk.free_bytes === null ? "unknown" : Schema.formatBytes(page.disk.free_bytes))
                : ""
        }
        Repeater {
            model: page.partitions
            InstallerText {
                required property var modelData
                tone: "muted"
                text: page.partitionLine(modelData)
            }
        }
        InstallerText {
            visible: page.disk !== null && page.partitions.length === 0
            text: "No partitions were found on this disk."
        }

        InstallerText {
            visible: page.disk !== null
            tone: "danger"
            text: "Everything on " + (page.disk ? page.disk.path : "") + " will be erased when you install with erase. Bingux cannot undo this."
        }
        InstallerText {
            visible: page.otherOsOnDisk
            tone: "warning"
            text: "This disk holds " + page.otherOs.join(", ") + "."
        }
        InstallerSwitchRow {
            visible: page.otherOsOnDisk && page.mode === "erase"
            label: "Erase the other system too"
            checked: InstallerState.otherOsAllow
            onToggled: value => InstallerState.otherOsAllow = value
        }

        InstallerText {
            tone: "text"
            text: "How to use the disk"
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null
            text: "Erase the whole disk"
            selected: page.mode === "erase"
            ticked: selected
            onClicked: InstallerState.storageMode = "erase"
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null && page.freeSpaceOffered && page.freeSpaceUsable
            text: !page.freeSpaceOffered ? "Use free space (not in this installer build yet)"
                : !page.freeSpaceUsable ? "Use free space (no unallocated space of 8 GiB or more)"
                : "Use free space"
            selected: page.mode === "free_space"
            ticked: selected
            onClicked: {
                InstallerState.storageMode = "free_space";
                InstallerState.shrinkPartition = "";
            }
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null && page.alongsideOffered && page.partitions.some(p => page.shrinkProblem(p) === "")
            text: !page.alongsideOffered ? "Install alongside another system (not in this installer build yet)"
                : "Install alongside another system (free " + Schema.formatBytes(page.keepForBingux) + " by resizing a partition)"
            selected: page.mode === "alongside"
            ticked: selected
            onClicked: {
                const first = page.partitions.find(p => page.shrinkProblem(p) === "");
                if (first)
                    page.choosePartition(first);
            }
        }
        Repeater {
            model: page.alongsideOffered ? page.partitions : []
            SessionRow {
                required property var modelData
                readonly property string problem: page.shrinkProblem(modelData)
                width: parent.width
                enabled: problem === ""
                text: "Resize " + modelData.path + " (" + (modelData.fstype || "no filesystem")
                    + ", " + Schema.formatBytes(modelData.size_bytes) + ")" + (problem !== "" ? "   (" + problem + ")" : "")
                selected: page.mode === "alongside" && InstallerState.shrinkPartition === modelData.path
                ticked: selected
                onClicked: page.choosePartition(modelData)
            }
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Partition by hand (no partition editor in this installer yet)"
        }
    }
}
