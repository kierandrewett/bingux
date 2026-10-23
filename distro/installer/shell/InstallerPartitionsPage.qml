import QtQuick
import "InstallerSchema.js" as Schema

// Partition by hand (storage mode manual). Each row is one partition of the
// plan: the role it plays, what happens to it (reuse, format or create) and the
// device it uses. Role, action, device and file system changes replace the
// list; a typed size changes its row in place and calls touchManual(), so the
// field keeps focus. Root takes the unallocated space left after the other new
// partitions until the user types its size.
InstallerPage {
    id: page

    title: "Partitions"
    bodyHeight: 600
    primaryText: "Continue"
    primaryEnabled: disk !== null && problems.length === 0

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

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            visible: page.disk !== null
            tone: "text"
            text: "Partitions on " + (page.disk ? page.disk.path : "") + ". Unallocated: "
                + Schema.formatBytes(page.unallocatedBytes) + ", new partitions: " + Schema.formatBytes(page.newBytes) + "."
        }
        InstallerText {
            tone: "warning"
            text: "Partitions marked format lose their data. Partitions marked keep are left as they are."
        }

        Repeater {
            model: InstallerState.manualEntries
            Column {
                id: row
                required property var modelData
                required property int index
                width: parent.width
                spacing: 6

                Flow {
                    width: parent.width
                    spacing: Theme.gap
                    ActionButton {
                        flat: true
                        text: "Role: " + (Schema.ROLE_NAMES[row.modelData.role] || row.modelData.role)
                        onClicked: page.cycleRole(row.index)
                    }
                    ActionButton {
                        flat: true
                        text: page.actionLabel(row.modelData.action)
                        onClicked: page.cycleAction(row.index)
                    }
                    ActionButton {
                        flat: true
                        visible: row.modelData.action !== "create"
                        text: page.deviceLabel(row.modelData)
                        onClicked: page.cycleDevice(row.index)
                    }
                    ActionButton {
                        flat: true
                        visible: row.modelData.role === "home" && row.modelData.action !== "reuse"
                        text: "File system: " + row.modelData.fs
                        onClicked: page.cycleFilesystem(row.index)
                    }
                    ActionButton {
                        flat: true
                        text: "Remove"
                        onClicked: page.removeEntry(row.index)
                    }
                }
                InstallerField {
                    visible: row.modelData.action === "create"
                    caption: "Size (GiB) of the new " + (Schema.ROLE_NAMES[row.modelData.role] || row.modelData.role)
                    text: row.modelData.sizeText
                    placeholderText: "GiB"
                    onEdited: page.setSize(row.index, text)
                }
                InstallerText {
                    tone: "muted"
                    text: page.changeText(row.modelData)
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
