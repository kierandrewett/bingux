import QtQuick
import "InstallerSchema.js" as Schema

// Partition by hand (storage mode manual). Each row is one partition of the
// plan: the role it plays, what happens to it (reuse, format or create) and the
// device it uses. Role, action, device and file system changes replace the
// list; a typed size changes its row in place and calls touchManual(), so the
// field keeps focus. Root takes the unallocated space left after the other new
// partitions until the user types its size. The bar above the table shows how
// the disk is divided; each row is a card with the same colour as its segment.
InstallerPage {
    id: page

    title: "Partitions"
    lead: "Choose what each partition does. Format erases it; keep leaves it alone."
    fullBody: true
    bodyHeight: 600
    primaryText: "Continue"
    primaryEnabled: disk !== null && problems.length === 0

    readonly property real us: InstallerState.uiScale
    readonly property var disk: InstallerState.selectedDisk
    readonly property var partitions: disk && disk.partitions ? disk.partitions : []
    readonly property var roles: InstallerState.uefi
        ? ["esp", "root", "home", "swap"]
        : ["bios_boot", "boot", "root", "home", "swap"]
    readonly property var problems: InstallerState.manualProblems()
    readonly property var addable: {
        const present = InstallerState.manualEntries.map(e => e.role);
        return roles.filter(role => present.indexOf(role) < 0);
    }
    readonly property real unallocatedBytes: disk && disk.free_bytes ? disk.free_bytes : 0
    readonly property real newBytes: newTotal()

    // Default size in GiB, as text, for a new partition of each role (root is computed).
    readonly property var sizeDefaults: ({esp: "0.5", bios_boot: "0.0625", boot: "1", home: "10", swap: "4"})

    // Segment colours: the bar and the row stripes share them.
    readonly property var segmentColors: ["#4f8cff", "#3fb68b", "#e0a526", "#b07cf0", "#e0567a", "#5cc8d8"]

    // Table columns: every cell in a row is cellHeight tall. The device column takes what is left of the row.
    readonly property int cellHeight: Math.round(46 * us)
    readonly property int colRole: Math.round(200 * us)
    readonly property int colAction: Math.round(170 * us)
    readonly property int colFs: Math.round(150 * us)
    readonly property int colSize: Math.round(130 * us)
    readonly property int colRemove: Math.round(104 * us)
    readonly property int colGaps: Theme.gap * 5
    readonly property int colFixed: colRole + colAction + colFs + colSize + colRemove + colGaps
    // Text inside a cell sits at the same inset as the ActionButton labels, so headers and cells line up.
    readonly property int cellInset: Theme.padding + Math.round(14 * us)
    readonly property int colDeviceMin: Math.round(120 * us)

    // Start the list when the page opens for manual storage with nothing chosen yet.
    onActiveChanged: {
        if (active && InstallerState.storageMode === "manual" && disk !== null && InstallerState.manualEntries.length === 0)
            InstallerState.manualEntries = defaultEntries();
    }

    function primary() {
        page.wizard.go("storage");
    }

    // The disk's partitions that no other row uses. `others` is the list without the row being set.
    function freeExisting(others) {
        const used = {};
        others.forEach(e => {
            if (e.device)
                used[e.device] = true;
        });
        return partitions.filter(p => !used[p.path]);
    }

    // Whether an existing partition can take the role: the EFI partition is FAT, home a Linux file system.
    function fitsRole(role, part) {
        if (role === "esp")
            return part.fstype === "vfat";
        if (role === "home")
            return Schema.HOME_FILESYSTEMS.indexOf(part.fstype) >= 0;
        return true;
    }

    // Reuse an EFI partition already on the disk when one fits; otherwise create one.
    function preferredAction(role, others) {
        if (role === "esp" && freeExisting(others).some(p => fitsRole(role, p)))
            return "reuse";
        return "create";
    }

    // GiB text for a new partition of role. Root takes the unallocated space the other new partitions leave.
    function defaultSize(role, others) {
        if (role !== "root")
            return sizeDefaults[role];
        let left = unallocatedBytes;
        others.forEach(e => {
            if (e.action === "create")
                left -= InstallerState.entrySizeBytes(e);
        });
        return String(Math.max(1, Math.floor(left / InstallerState.gib)));
    }

    // One row for role and action, with the device and size the action needs.
    // `others` are the other rows; `old` is the row it replaces, or null.
    function buildEntry(role, action, others, old) {
        const entry = {
            role: role,
            action: action,
            device: "",
            sizeText: "",
            typed: false,
            fs: old && old.fs ? old.fs : "ext4"
        };
        if (action === "create") {
            entry.device = Schema.partitionPath(disk.path, Schema.freePartitionNumber(disk, others));
            const keep = old && old.action === "create" && old.role === role && old.typed;
            entry.sizeText = keep ? old.sizeText : defaultSize(role, others);
            entry.typed = keep;
            return entry;
        }
        const free = freeExisting(others);
        const fitting = free.filter(p => action === "format" || fitsRole(role, p));
        const kept = old && old.device && free.some(p => p.path === old.device);
        if (kept)
            entry.device = old.device;
        else if (fitting.length > 0)
            entry.device = fitting[0].path;
        else if (free.length > 0)
            entry.device = free[0].path;
        return entry;
    }

    // Stores a new list. Root's size follows the other new partitions until the user types one.
    function commit(list) {
        const index = list.findIndex(e => e.role === "root");
        if (index >= 0 && list[index].action === "create" && !list[index].typed) {
            const others = list.filter((e, i) => i !== index);
            list[index] = Object.assign({}, list[index], {sizeText: defaultSize("root", others)});
        }
        InstallerState.manualEntries = list;
    }

    function defaultEntries() {
        let list = [];
        const starts = InstallerState.uefi ? ["esp", "root"] : ["bios_boot", "root"];
        starts.forEach(role => {
            list = list.concat([buildEntry(role, preferredAction(role, list), list, null)]);
        });
        return list;
    }

    function setEntry(index, role, action) {
        const list = InstallerState.manualEntries.slice();
        const others = list.filter((e, i) => i !== index);
        list[index] = buildEntry(role, action, others, list[index]);
        commit(list);
    }

    function addRole(role) {
        const list = InstallerState.manualEntries.slice();
        const next = buildEntry(role, preferredAction(role, list), list, null);
        commit(list.concat([next]));
    }

    function removeEntry(index) {
        const list = InstallerState.manualEntries.slice();
        list.splice(index, 1);
        commit(list);
    }

    function cycleRole(index) {
        const entry = InstallerState.manualEntries[index];
        const next = roles[(roles.indexOf(entry.role) + 1) % roles.length];
        const actions = Schema.ROLE_ACTIONS[next];
        const action = actions.indexOf(entry.action) >= 0 ? entry.action : actions[0];
        setEntry(index, next, action);
    }

    function cycleAction(index) {
        const entry = InstallerState.manualEntries[index];
        const actions = Schema.ROLE_ACTIONS[entry.role];
        setEntry(index, entry.role, actions[(actions.indexOf(entry.action) + 1) % actions.length]);
    }

    // Moves a reuse or format row to the next partition of the disk that no other row uses.
    function cycleDevice(index) {
        const list = InstallerState.manualEntries.slice();
        const entry = list[index];
        const free = freeExisting(list.filter((e, i) => i !== index)).map(p => p.path);
        if (free.length === 0)
            return;
        const next = free[(free.indexOf(entry.device) + 1) % free.length];
        list[index] = Object.assign({}, entry, {device: next});
        commit(list);
    }

    function cycleFilesystem(index) {
        const list = InstallerState.manualEntries.slice();
        const entry = list[index];
        const fs = Schema.HOME_FILESYSTEMS;
        list[index] = Object.assign({}, entry, {fs: fs[(fs.indexOf(entry.fs) + 1) % fs.length]});
        commit(list);
    }

    // A typed size: the row changes in place, so the field keeps focus.
    function setSize(index, text) {
        const entry = InstallerState.manualEntries[index];
        entry.sizeText = text;
        entry.typed = true;
        InstallerState.touchManual();
    }

    function newTotal() {
        InstallerState.manualRevision;
        let total = 0;
        InstallerState.manualEntries.forEach(e => {
            if (e.action === "create")
                total += InstallerState.entrySizeBytes(e);
        });
        return total;
    }

    // Bytes a row takes on the disk: the size typed for a new partition, or the size of the existing one.
    function entryBytes(entry) {
        InstallerState.manualRevision;
        if (entry.action === "create")
            return InstallerState.entrySizeBytes(entry);
        const part = Schema.existingPartition(disk, entry.device);
        return part && part.size_bytes ? part.size_bytes : 0;
    }

    // The share of the disk a segment takes, as a width in the bar. Tiny segments keep 4 px so they stay visible.
    function segmentWidth(bytes, total, barWidth) {
        if (!total || !bytes)
            return 0;
        return Math.max(4, Math.round(barWidth * bytes / total));
    }

    // A file system choice is only meaningful for a home partition; the root file system is set on the Storage page.
    function fsCaption(entry) {
        if (entry.role === "home")
            return entry.fs;
        return entry.role === "root" ? "Set on Storage" : "Fixed";
    }

    function actionLabel(action) {
        return action === "reuse" ? "Keep (reuse)" : action === "format" ? "Format" : "Create new";
    }

    function deviceLabel(entry) {
        if (entry.device === "")
            return "Choose a partition";
        const part = Schema.existingPartition(disk, entry.device);
        return entry.device + (part && part.fstype ? " (" + part.fstype + ")" : "");
    }

    // What the row does to the disk, in one sentence.
    function changeText(entry) {
        if (entry.action === "create")
            return "Creates " + entry.device + ", " + Schema.formatBytes(InstallerState.entrySizeBytes(entry)) + ".";
        if (entry.action === "format")
            return "Formats " + entry.device + "; everything on it is erased.";
        return "Keeps " + entry.device + " and the files on it.";
    }

    InstallerScroll {
        spacing: Math.round(12 * page.us)

        InstallerText {
            visible: page.disk !== null
            tone: "text"
            text: "Partitions on " + (page.disk ? page.disk.path : "") + ". Unallocated: "
                + Schema.formatBytes(page.unallocatedBytes) + ", new partitions: " + Schema.formatBytes(page.newBytes) + "."
        }

        // The disk as a bar: one segment per row, then the unallocated space.
        Row {
            id: sizeBar
            visible: page.disk !== null
            width: parent.width
            height: Math.round(22 * page.us)
            clip: true
            readonly property real total: page.disk ? page.disk.size_bytes : 0

            Repeater {
                model: InstallerState.manualEntries
                Rectangle {
                    required property var modelData
                    required property int index
                    width: page.segmentWidth(page.entryBytes(modelData), sizeBar.total, sizeBar.width)
                    height: sizeBar.height
                    color: page.segmentColors[index % page.segmentColors.length]
                }
            }
            Rectangle {
                width: page.segmentWidth(page.unallocatedBytes, sizeBar.total, sizeBar.width)
                height: sizeBar.height
                color: Qt.rgba(1, 1, 1, 0.22)
            }
        }

        InstallerText {
            tone: "warning"
            text: "Partitions marked format lose their data. Partitions marked keep are left as they are."
        }

        // Table header, aligned with the cells below.
        Row {
            visible: InstallerState.manualEntries.length > 0
            x: Math.round(16 * page.us)
            width: parent.width - Math.round(28 * page.us)
            spacing: Theme.gap
            Text {
                width: page.colRole
                leftPadding: page.cellInset
                text: "Role"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(15 * page.us)
            }
            Text {
                width: page.colAction
                leftPadding: page.cellInset
                text: "Action"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(15 * page.us)
            }
            Text {
                width: Math.max(page.colDeviceMin, parent.width - page.colFixed)
                leftPadding: page.cellInset
                text: "Device"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(15 * page.us)
            }
            Text {
                width: page.colFs
                leftPadding: page.cellInset
                text: "File system"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(15 * page.us)
            }
            Text {
                width: page.colSize
                leftPadding: page.cellInset
                text: "Size (GiB)"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Math.round(15 * page.us)
            }
            Item {
                width: page.colRemove
                height: 1
            }
        }

        // One card per row.
        Repeater {
            model: InstallerState.manualEntries
            Rectangle {
                id: card
                required property var modelData
                required property int index
                width: parent.width
                height: cardColumn.implicitHeight + Math.round(20 * page.us)
                radius: Theme.radius
                color: Qt.rgba(1, 1, 1, 0.06)
                border.width: 1
                border.color: Qt.rgba(1, 1, 1, 0.12)

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.margins: 1
                    width: Math.round(6 * page.us)
                    radius: Theme.radius
                    color: page.segmentColors[card.index % page.segmentColors.length]
                }

                Column {
                    id: cardColumn
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.leftMargin: Math.round(16 * page.us)
                    anchors.rightMargin: Math.round(12 * page.us)
                    anchors.topMargin: Math.round(10 * page.us)
                    spacing: Math.round(6 * page.us)

                    Row {
                        width: parent.width
                        spacing: Theme.gap

                        ActionButton {
                            width: page.colRole
                            height: page.cellHeight
                            flat: true
                            alignLeft: true
                            text: Schema.ROLE_NAMES[card.modelData.role] || card.modelData.role
                            onClicked: page.cycleRole(card.index)
                        }
                        ActionButton {
                            width: page.colAction
                            height: page.cellHeight
                            flat: true
                            alignLeft: true
                            text: page.actionLabel(card.modelData.action)
                            onClicked: page.cycleAction(card.index)
                        }
                        // A new partition has a device of its own; an existing one is chosen from the disk.
                        Item {
                            width: Math.max(page.colDeviceMin, parent.width - page.colFixed)
                            height: page.cellHeight
                            ActionButton {
                                visible: card.modelData.action !== "create"
                                anchors.fill: parent
                                flat: true
                                alignLeft: true
                                text: page.deviceLabel(card.modelData)
                                onClicked: page.cycleDevice(card.index)
                            }
                            Text {
                                visible: card.modelData.action === "create"
                                anchors.verticalCenter: parent.verticalCenter
                                leftPadding: page.cellInset
                                text: card.modelData.device + " (new)"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSize
                            }
                        }
                        // A home partition takes a file system; the others show where theirs comes from.
                        Item {
                            width: page.colFs
                            height: page.cellHeight
                            ActionButton {
                                visible: card.modelData.role === "home" && card.modelData.action !== "reuse"
                                anchors.fill: parent
                                flat: true
                                alignLeft: true
                                text: card.modelData.fs
                                onClicked: page.cycleFilesystem(card.index)
                            }
                            Text {
                                visible: !(card.modelData.role === "home" && card.modelData.action !== "reuse")
                                anchors.verticalCenter: parent.verticalCenter
                                leftPadding: page.cellInset
                                text: page.fsCaption(card.modelData)
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSize
                            }
                        }
                        Item {
                            width: page.colSize
                            height: page.cellHeight
                            InstallerField {
                                visible: card.modelData.action === "create"
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width
                                placeholderText: "GiB"
                                text: card.modelData.sizeText
                                onEdited: page.setSize(card.index, text)
                            }
                            Text {
                                visible: card.modelData.action !== "create"
                                anchors.verticalCenter: parent.verticalCenter
                                leftPadding: page.cellInset
                                text: Schema.formatBytes(page.entryBytes(card.modelData))
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSize
                            }
                        }
                        ActionButton {
                            width: page.colRemove
                            height: page.cellHeight
                            flat: true
                            text: "Remove"
                            onClicked: page.removeEntry(card.index)
                        }
                    }

                    InstallerText {
                        tone: "muted"
                        text: page.changeText(card.modelData)
                    }
                }
            }
        }

        InstallerText {
            visible: InstallerState.manualEntries.length === 0
            text: "No partitions yet. Add the partitions Bingux needs below."
        }

        InstallerText {
            visible: page.addable.length > 0
            tone: "text"
            text: "Add a partition"
        }
        Flow {
            visible: page.addable.length > 0
            width: parent.width
            spacing: Theme.gap
            Repeater {
                model: page.addable
                ActionButton {
                    required property var modelData
                    height: page.cellHeight
                    flat: true
                    text: Schema.ROLE_NAMES[modelData] || modelData
                    onClicked: page.addRole(modelData)
                }
            }
        }

        Repeater {
            model: page.problems
            InstallerText {
                required property var modelData
                tone: "danger"
                text: modelData
            }
        }
    }
}
