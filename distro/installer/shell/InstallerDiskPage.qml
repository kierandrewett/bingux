import QtQuick
import "InstallerSchema.js" as Schema

// Disk and how Bingux uses it. The probe's partitions and capabilities decide
// what is offered: a way of using the disk is enabled only when this installer
// build implements it (capabilities.storage_modes) and the disk can take it.
// The daemon reads the partition table for every mode but erase, so those
// modes need GPT.
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

    // Room left for Bingux when installing alongside: the default new size of the partition.
    readonly property real keepForBingux: 20 * InstallerState.gib
    // The least Bingux can be installed into; alongside leaves at least this much free.
    readonly property real minFreeForBingux: Schema.MIN_DISK_BYTES

    readonly property var disk: InstallerState.selectedDisk
    readonly property var otherOs: disk && disk.other_os ? disk.other_os : []
    readonly property bool otherOsOnDisk: otherOs.length > 0
    readonly property var partitions: disk && disk.partitions ? disk.partitions : []
    readonly property string mode: InstallerState.storageMode
    readonly property bool gpt: disk !== null && disk.partition_table === "gpt"
    readonly property var shrinkPart: partitionByPath(InstallerState.shrinkPartition)

    readonly property bool freeSpaceOffered: InstallerState.offers("storage_modes", "free_space")
    readonly property bool alongsideOffered: InstallerState.offers("storage_modes", "alongside")
    readonly property bool manualOffered: InstallerState.offers("storage_modes", "manual")
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
        if (mode === "alongside" && shrinkPart === null)
            return "no partition chosen";
        return "";
    }

    function partitionByPath(path) {
        for (const part of partitions)
            if (part.path === path)
                return part;
        return null;
    }

    function shrinkProblem(part) {
        if (part.shrinkable !== true)
            return "cannot be shrunk";
        if (part.size_bytes === null || part.min_bytes === null)
            return "size unknown";
        if (part.size_bytes - minFreeForBingux < part.min_bytes)
            return "not enough room to free " + Schema.formatBytes(minFreeForBingux);
        return "";
    }

    // Why a mode that needs the partition table is off; "" when it is available.
    function gptProblem() {
        return gpt ? "" : "needs a GPT partition table";
    }

    function chooseDisk(path) {
        InstallerState.diskPath = path;
        InstallerState.storageMode = "erase";
        InstallerState.shrinkPartition = "";
        InstallerState.manualEntries = [];
    }

    function chooseMode(value) {
        InstallerState.storageMode = value;
        if (value !== "alongside")
            InstallerState.shrinkPartition = "";
    }

    function choosePartition(part) {
        const top = part.size_bytes - minFreeForBingux;
        const start = Math.max(part.min_bytes, Math.min(top, part.size_bytes - keepForBingux));
        InstallerState.storageMode = "alongside";
        InstallerState.shrinkPartition = part.path;
        InstallerState.shrinkBytes = start;
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
        page.wizard.go(mode === "manual" ? "partitions" : "storage");
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
            visible: page.disk !== null && page.mode === "erase"
            tone: "danger"
            text: "Everything on " + (page.disk ? page.disk.path : "") + " will be erased when you install. Bingux cannot undo this."
        }
        InstallerText {
            visible: page.disk !== null && page.mode === "manual"
            tone: "warning"
            text: "You choose on the next page which partitions are formatted. Only those lose their data."
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
            onClicked: page.chooseMode("erase")
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null && page.freeSpaceOffered && page.gpt && page.freeSpaceUsable
            text: !page.freeSpaceOffered ? "Use free space (not in this installer build yet)"
                : !page.gpt && page.disk !== null ? "Use free space (" + page.gptProblem() + ")"
                : !page.freeSpaceUsable ? "Use free space (no unallocated space of 8 GiB or more)"
                : "Use free space"
            selected: page.mode === "free_space"
            ticked: selected
            onClicked: page.chooseMode("free_space")
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null && page.alongsideOffered && page.gpt && page.partitions.some(p => page.shrinkProblem(p) === "")
            text: !page.alongsideOffered ? "Install alongside another system (not in this installer build yet)"
                : !page.gpt && page.disk !== null ? "Install alongside another system (" + page.gptProblem() + ")"
                : "Install alongside another system (keep at least " + Schema.formatBytes(page.minFreeForBingux) + " free by resizing a partition)"
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
                enabled: problem === "" && page.gpt
                text: "Resize " + modelData.path + " (" + (modelData.fstype || "no filesystem")
                    + ", " + Schema.formatBytes(modelData.size_bytes) + ")" + (problem !== "" ? "   (" + problem + ")" : "")
                selected: page.mode === "alongside" && InstallerState.shrinkPartition === modelData.path
                ticked: selected
                onClicked: page.choosePartition(modelData)
            }
        }
        InstallerSlider {
            visible: page.mode === "alongside" && page.shrinkPart !== null
            enabled: page.shrinkPart !== null
            from: page.shrinkPart ? page.shrinkPart.min_bytes : 0
            to: page.shrinkPart ? page.shrinkPart.size_bytes - page.minFreeForBingux : 1
            value: InstallerState.shrinkBytes
            caption: page.shrinkPart ? "New size of " + page.shrinkPart.path : ""
            valueText: page.shrinkPart
                ? Schema.formatBytes(InstallerState.shrinkBytes) + "   " + Schema.formatBytes(page.shrinkPart.size_bytes - InstallerState.shrinkBytes) + " free for Bingux"
                : ""
            onMoved: v => InstallerState.shrinkBytes = Math.round(v)
        }
        SessionRow {
            width: parent.width
            enabled: page.disk !== null && page.manualOffered && page.gpt
            text: !page.manualOffered ? "Partition by hand (not in this installer build yet)"
                : !page.gpt && page.disk !== null ? "Partition by hand (" + page.gptProblem() + ")"
                : "Partition by hand"
            selected: page.mode === "manual"
            ticked: selected
            onClicked: page.chooseMode("manual")
        }
    }
}
