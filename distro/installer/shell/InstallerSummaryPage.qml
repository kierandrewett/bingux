import QtQuick
import "InstallerSchema.js" as Schema

// Lists exactly what the install will do, then asks for the disk to be
// confirmed. Install starts from here; the progress page follows the result.
InstallerPage {
    id: page

    title: "Ready to install"
    bodyHeight: 600
    primaryText: InstallerState.storageMode === "erase" ? "Erase and install" : page.manualFormats().length > 0 ? "Format and install" : "Install"
    primaryEnabled: confirmedDisk !== "" && confirmedDisk === InstallerState.diskPath

    // InstallerClient, which starts the install.
    property var client: null
    property string confirmedDisk: ""

    readonly property var disk: InstallerState.selectedDisk
    readonly property var rows: buildRows()

    function storageText() {
        const d = page.disk;
        if (!d)
            return "";
        if (InstallerState.storageMode === "free_space")
            return "Install into the unallocated space on " + d.path + "; existing partitions are kept";
        if (InstallerState.storageMode === "alongside") {
            const part = (d.partitions || []).find(p => p.path === InstallerState.shrinkPartition);
            const from = part && part.size_bytes !== null ? Schema.formatBytes(part.size_bytes) : "its current size";
            return "Resize " + InstallerState.shrinkPartition + " from " + from + " to "
                + Schema.formatBytes(InstallerState.shrinkBytes) + " and install alongside it; its data is kept";
        }
        if (InstallerState.storageMode === "manual")
            return "Change the partitions on " + d.path + " as listed below";
        const other = d.other_os || [];
        return "Erase everything on " + d.path + (other.length ? " (including " + other.join(", ") + ")" : "");
    }

    // Manual mode: one row per partition change, from the Partitions page list.
    function changeRows() {
        InstallerState.manualRevision;
        if (InstallerState.storageMode !== "manual")
            return [];
        return InstallerState.manualEntries.map(e => {
            const fs = e.role === "home" && e.action !== "reuse" ? " (" + e.fs + ")" : "";
            const name = (Schema.ROLE_NAMES[e.role] || e.role) + fs;
            let text;
            if (e.action === "create")
                text = "Create " + name + " on " + e.device + ", " + Schema.formatBytes(InstallerState.entrySizeBytes(e)) + " from the unallocated space";
            else if (e.action === "format")
                text = "Format " + e.device + " as " + name + "; its data is erased";
            else
                text = "Use " + e.device + " as " + name + " without formatting";
            return {label: "Partition", value: text};
        });
    }

    // Entries that format a partition: the data on them is erased.
    function manualFormats() {
        if (InstallerState.storageMode !== "manual")
            return [];
        return InstallerState.manualEntries.filter(e => e.action === "format");
    }
    function destructive() {
        return InstallerState.storageMode === "erase" || manualFormats().length > 0;
    }
    function destructiveText() {
        if (InstallerState.storageMode === "erase")
            return "Installing erases " + (page.disk ? page.disk.path : "") + " completely.";
        return "Formatting erases the data on " + manualFormats().map(e => e.device).join(", ") + ".";
    }

    function swapText() {
        switch (InstallerState.effectiveSwap()) {
        case "zram": return "zram (compressed in RAM)";
        case "file": return "Swap file of " + InstallerState.swapGiB + " GiB on the root file system";
        case "partition": return InstallerState.storageMode === "manual" ? "Swap partition (listed below)" : "Swap partition of " + InstallerState.swapGiB + " GiB";
        default: return "None";
        }
    }

    function buildRows() {
        const list = [];
        if (page.disk) {
            list.push({label: "Disk", value: page.disk.path + " (" + Schema.formatBytes(page.disk.size_bytes) + ", " + (page.disk.model || "unknown model") + ")"});
            list.push({label: "Storage", value: page.storageText()});
            page.changeRows().forEach(row => list.push(row));
        }
        list.push({label: "File system", value: InstallerState.filesystem + (InstallerState.encryption ? ", encrypted with LUKS2" : ", not encrypted")});
        list.push({label: "Swap", value: page.swapText()});
        list.push({label: "Computer name", value: InstallerState.hostname});
        list.push({label: "Time zone", value: InstallerState.timezone.replace(/_/g, " ")});
        list.push({label: "Clock", value: InstallerState.ntp ? "Set automatically (systemd-timesyncd)" : "Not set automatically"});
        list.push({label: "Language", value: InstallerState.locale});
        list.push({label: "Keyboard", value: InstallerState.keymap});
        list.push({label: "User", value: (InstallerState.fullName !== "" ? InstallerState.fullName + " (" + InstallerState.username + ")" : InstallerState.username) + ", administrator"
            + (InstallerState.autoLogin ? ", logs in automatically" : "")});
        list.push({label: "Root account", value: InstallerState.rootPasswordSet ? "Password set" : "Locked"});
        return list;
    }

    function primary() {
        if (!primaryEnabled)
            return;
        page.client.install(InstallerState.planObject());
        page.wizard.go("progress");
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            tone: "danger"
            visible: page.disk !== null && page.destructive()
            text: page.destructiveText()
        }
        Repeater {
            model: page.rows
            Row {
                required property var modelData
                width: parent.width
                spacing: Theme.gap
                Text {
                    width: parent.width * 0.36
                    text: modelData.label
                    color: Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall + 1
                }
                Text {
                    width: parent.width * 0.64 - Theme.gap
                    text: modelData.value
                    color: Theme.text
                    wrapMode: Text.WordWrap
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall + 1
                }
            }
        }
        InstallerSwitchRow {
            visible: page.disk !== null
            label: InstallerState.storageMode === "erase"
                ? "I understand that everything on " + (page.disk ? page.disk.path : "the disk") + " will be erased."
                : "I understand the changes above will be made to " + (page.disk ? page.disk.path : "the disk") + "."
            checked: page.confirmedDisk !== "" && page.confirmedDisk === InstallerState.diskPath
            onToggled: value => page.confirmedDisk = value ? InstallerState.diskPath : ""
        }
    }
}
