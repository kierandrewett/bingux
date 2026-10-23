import QtQuick
import "InstallerSchema.js" as Schema

// Disk and how Bingux uses it. The probe's partitions and capabilities decide
// what is offered: a way of using the disk is enabled only when this installer
// build implements it (capabilities.storage_modes) and the disk can take it.
// The daemon reads the partition table for every mode but erase, so those
// modes need GPT. Left: one card per disk, with its partitions as a bar and
// chips. Right: how to use the chosen disk, one large row per way.
InstallerPage {
    id: page

    title: "Disk"
    lead: "Choose the disk Bingux goes on, then how to use it."
    fullBody: true
    bodyHeight: 600

    onActiveChanged: {
        if (active && client)
            client.probe();
    }

    // InstallerClient, used to probe again.
    property var client: null

    readonly property real us: InstallerState.uiScale

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

    // Segment colours for the partition bars; free space is drawn grey.
    readonly property var segmentColors: ["#4f8cff", "#3fb68b", "#e0a526", "#b07cf0", "#e0567a", "#5cc8d8"]

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

    function shortPath(path) {
        return path.replace("/dev/", "");
    }

    // The chip text for one partition: name, file system and size.
    function chipText(part) {
        return shortPath(part.path) + "  " + (part.fstype || "no fs") + "  " + Schema.formatBytes(part.size_bytes);
    }

    // The share of the disk a partition takes, as a width in the bar.
    function segmentWidth(bytes, total, barWidth) {
        if (!total || !bytes)
            return 0;
        return Math.max(4, Math.round(barWidth * bytes / total));
    }

    function primary() {
        page.wizard.go(mode === "manual" ? "partitions" : "storage");
    }

    // Reason an option is off, or "" when it is on. `base` is the reason that applies to every option.
    function freeSpaceReason() {
        if (disk === null)
            return "Choose a disk first.";
        if (!freeSpaceOffered)
            return "Not in this installer build yet.";
        if (!gpt)
            return "Needs a GPT partition table.";
        if (!freeSpaceUsable)
            return "Needs 8 GiB or more of unallocated space.";
        return "";
    }

    function alongsideReason() {
        if (disk === null)
            return "Choose a disk first.";
        if (!alongsideOffered)
            return "Not in this installer build yet.";
        if (!gpt)
            return "Needs a GPT partition table.";
        if (!partitions.some(p => shrinkProblem(p) === ""))
            return "No partition can shrink to leave 8 GiB free.";
        return "";
    }

    function manualReason() {
        if (disk === null)
            return "Choose a disk first.";
        if (!manualOffered)
            return "Not in this installer build yet.";
        if (!gpt)
            return "Needs a GPT partition table.";
        return "";
    }

    InstallerScroll {
        spacing: Math.round(14 * page.us)

        Row {
            width: parent.width
            spacing: Theme.gap

            // Left: the disks.
            Column {
                id: leftColumn
                width: Math.round((parent.width - Theme.gap) * 0.42)
                spacing: Math.round(10 * page.us)

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
                    Rectangle {
                        id: card
                        required property var modelData
                        readonly property string problem: page.diskProblem(modelData)
                        readonly property bool chosen: InstallerState.diskPath === modelData.path
                        readonly property var parts: modelData.partitions ? modelData.partitions : []

                        width: parent.width
                        height: cardColumn.implicitHeight + Math.round(32 * page.us)
                        radius: Theme.radius
                        opacity: problem === "" ? 1 : 0.7
                        color: chosen ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.16) : Qt.rgba(1, 1, 1, 0.06)
                        border.width: chosen ? 3 : 1
                        border.color: chosen ? Theme.accent : Qt.rgba(1, 1, 1, 0.14)
                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.path

                        MouseArea {
                            anchors.fill: parent
                            enabled: card.problem === ""
                            cursorShape: Qt.PointingHandCursor
                            onClicked: page.chooseDisk(card.modelData.path)
                        }

                        Column {
                            id: cardColumn
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: Math.round(16 * page.us)
                            spacing: Math.round(8 * page.us)

                            Row {
                                width: parent.width
                                spacing: Theme.gap

                                Column {
                                    width: parent.width - (selectedPill.visible ? selectedPill.width + Theme.gap : 0)
                                    spacing: 2
                                    Text {
                                        width: parent.width
                                        elide: Text.ElideRight
                                        text: card.modelData.path
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Math.round(22 * page.us)
                                        font.weight: Font.DemiBold
                                    }
                                    Text {
                                        width: parent.width
                                        elide: Text.ElideRight
                                        text: (card.modelData.model || "unknown model") + "   " + Schema.formatBytes(card.modelData.size_bytes)
                                            + "   " + (card.modelData.partition_table ? card.modelData.partition_table.toUpperCase() : "no table")
                                        color: Theme.muted
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Math.round(15 * page.us)
                                    }
                                }
                                Rectangle {
                                    id: selectedPill
                                    visible: card.chosen
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: pillText.implicitWidth + Math.round(20 * page.us)
                                    height: Math.round(28 * page.us)
                                    radius: height / 2
                                    color: Theme.accent
                                    Text {
                                        id: pillText
                                        anchors.centerIn: parent
                                        text: "Selected"
                                        color: "#ffffff"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Math.round(14 * page.us)
                                        font.weight: Font.DemiBold
                                    }
                                }
                            }

                            // Partitions as a proportional bar; free space is the grey end.
                            Row {
                                id: bar
                                width: parent.width
                                height: Math.round(16 * page.us)
                                spacing: 0
                                clip: true
                                readonly property real total: card.modelData.size_bytes || 0
                                Repeater {
                                    model: card.parts
                                    Rectangle {
                                        required property var modelData
                                        required property int index
                                        width: page.segmentWidth(modelData.size_bytes, bar.total, bar.width)
                                        height: bar.height
                                        color: page.segmentColors[index % page.segmentColors.length]
                                    }
                                }
                                Rectangle {
                                    width: page.segmentWidth(card.modelData.free_bytes, bar.total, bar.width)
                                    height: bar.height
                                    color: Qt.rgba(1, 1, 1, 0.22)
                                }
                            }

                            // Partitions and what they hold, as chips.
                            Flow {
                                width: parent.width
                                spacing: Math.round(6 * page.us)
                                Repeater {
                                    model: card.parts
                                    Rectangle {
                                        required property var modelData
                                        height: Math.round(26 * page.us)
                                        width: chip.implicitWidth + Math.round(16 * page.us)
                                        radius: height / 2
                                        color: Qt.rgba(1, 1, 1, 0.1)
                                        Text {
                                            id: chip
                                            anchors.centerIn: parent
                                            text: page.chipText(modelData) + (modelData.os ? "  " + modelData.os : "")
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Math.round(14 * page.us)
                                        }
                                    }
                                }
                                Repeater {
                                    model: card.modelData.free_bytes ? [card.modelData.free_bytes] : []
                                    Rectangle {
                                        required property var modelData
                                        height: Math.round(26 * page.us)
                                        width: freeChip.implicitWidth + Math.round(16 * page.us)
                                        radius: height / 2
                                        color: "transparent"
                                        border.width: 1
                                        border.color: Qt.rgba(1, 1, 1, 0.3)
                                        Text {
                                            id: freeChip
                                            anchors.centerIn: parent
                                            text: "free " + Schema.formatBytes(modelData)
                                            color: Theme.muted
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Math.round(14 * page.us)
                                        }
                                    }
                                }
                                Text {
                                    visible: card.parts.length === 0
                                    text: "No partitions"
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Math.round(14 * page.us)
                                }
                            }

                            Text {
                                visible: card.problem !== ""
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: card.problem
                                color: Theme.danger
                                font.family: Theme.fontFamily
                                font.pixelSize: Math.round(15 * page.us)
                            }
                            Text {
                                visible: card.modelData.other_os && card.modelData.other_os.length > 0
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: "Holds " + (card.modelData.other_os || []).join(", ")
                                color: Theme.warning
                                font.family: Theme.fontFamily
                                font.pixelSize: Math.round(15 * page.us)
                            }
                        }
                    }
                }
            }

            // Right: how to use the chosen disk.
            Column {
                width: parent.width - leftColumn.width - Theme.gap
                spacing: Math.round(10 * page.us)

                Text {
                    width: parent.width
                    text: "How to use the disk"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(20 * page.us)
                    font.weight: Font.DemiBold
                }

                InstallerOption {
                    width: parent.width
                    enabled: page.disk !== null
                    opacity: enabled ? 1 : 0.5
                    text: "Erase the whole disk"
                    detail: page.disk !== null ? "Deletes everything on the disk. Bingux takes all of it." : "Choose a disk first."
                    selected: page.mode === "erase"
                    ticked: selected && page.disk !== null
                    onClicked: page.chooseMode("erase")
                }
                InstallerOption {
                    width: parent.width
                    enabled: page.disk !== null && page.freeSpaceOffered && page.gpt && page.freeSpaceUsable
                    opacity: enabled ? 1 : 0.5
                    text: "Use free space"
                    detail: page.freeSpaceReason() !== "" ? "Unavailable: " + page.freeSpaceReason()
                        : "Installs into unallocated space. Existing partitions stay."
                    selected: page.mode === "free_space"
                    ticked: selected
                    onClicked: page.chooseMode("free_space")
                }
                InstallerOption {
                    width: parent.width
                    enabled: page.disk !== null && page.alongsideOffered && page.gpt && page.partitions.some(p => page.shrinkProblem(p) === "")
                    opacity: enabled ? 1 : 0.5
                    text: "Install alongside another system"
                    detail: page.alongsideReason() !== "" ? "Unavailable: " + page.alongsideReason()
                        : "Shrinks a partition to make room. Keeps the other system."
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
                    Row {
                        required property var modelData
                        width: parent.width
                        spacing: 0
                        Item {
                            width: Math.round(28 * page.us)
                            height: 1
                        }
                        InstallerOption {
                            width: parent.width - Math.round(28 * page.us)
                            readonly property string problem: page.shrinkProblem(modelData)
                            enabled: problem === "" && page.gpt
                            opacity: enabled ? 1 : 0.5
                            text: "Shrink " + modelData.path
                            detail: problem !== "" ? "Unavailable: " + problem
                                : (modelData.fstype || "no filesystem") + ", " + Schema.formatBytes(modelData.size_bytes)
                            selected: page.mode === "alongside" && InstallerState.shrinkPartition === modelData.path
                            ticked: selected
                            onClicked: page.choosePartition(modelData)
                        }
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
                InstallerOption {
                    width: parent.width
                    enabled: page.disk !== null && page.manualOffered && page.gpt
                    opacity: enabled ? 1 : 0.5
                    text: "Partition by hand"
                    detail: page.manualReason() !== "" ? "Unavailable: " + page.manualReason()
                        : "Choose the partitions, sizes and what is formatted."
                    selected: page.mode === "manual"
                    ticked: selected
                    onClicked: page.chooseMode("manual")
                }

                InstallerNotice {
                    visible: page.disk !== null && page.mode === "erase"
                    tone: "danger"
                    title: "Erase " + (page.disk ? page.disk.path : "the disk") + " completely"
                    body: "Everything on it is deleted when you install. Bingux cannot undo this."
                }
                InstallerNotice {
                    visible: page.otherOsOnDisk
                    tone: "warning"
                    title: "This disk holds " + page.otherOs.join(", ")
                    body: page.mode === "erase"
                        ? "Erasing the disk removes it too. Turn on the switch below to allow that."
                        : "Other systems on this disk are left as they are in this mode."
                }
                InstallerSwitchRow {
                    visible: page.otherOsOnDisk && page.mode === "erase"
                    label: "Erase the other system too"
                    checked: InstallerState.otherOsAllow
                    onToggled: value => InstallerState.otherOsAllow = value
                }
                InstallerNotice {
                    visible: page.disk !== null && page.mode === "manual"
                    tone: "warning"
                    title: "Partitions are chosen next"
                    body: "On the next page you choose which partitions are formatted. Only those lose their data."
                }
                InstallerText {
                    visible: page.disk !== null && page.modeProblem() !== ""
                    tone: "danger"
                    text: "Cannot continue: " + page.modeProblem() + "."
                }
            }
        }
    }
}
