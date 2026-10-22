import QtQuick
import "InstallerSchema.js" as Schema

// Lists exactly what the install will do, then asks for the erase to be
// confirmed. Install starts from here; the progress page follows the result.
InstallerPage {
    id: page

    title: "Ready to install"
    bodyHeight: 560
    primaryText: "Erase and install"
    primaryEnabled: confirmedDisk !== "" && confirmedDisk === InstallerState.diskPath

    // InstallerClient, which starts the install.
    property var client: null
    property string confirmedDisk: ""

    readonly property var disk: InstallerState.selectedDisk
    readonly property var rows: buildRows()

    function buildRows() {
        const list = [];
        const d = page.disk;
        if (d) {
            list.push({label: "Disk", value: d.path + " (" + Schema.formatBytes(d.size_bytes) + ", " + (d.model || "unknown model") + ")"});
            const other = d.other_os || [];
            list.push({label: "Erased", value: "Everything on the disk" + (other.length ? " (including " + other.join(", ") + ")" : "")});
        }
        list.push({label: "File system", value: InstallerState.filesystem + (InstallerState.encryption ? ", encrypted with LUKS2" : ", not encrypted")});
        list.push({label: "Computer name", value: InstallerState.hostname});
        list.push({label: "Time zone", value: InstallerState.timezone.replace(/_/g, " ")});
        list.push({label: "Language", value: InstallerState.locale});
        list.push({label: "Keyboard", value: InstallerState.keymap});
        list.push({label: "User", value: InstallerState.fullName !== "" ? InstallerState.fullName + " (" + InstallerState.username + ")" : InstallerState.username + " (administrator)"});
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
            visible: page.disk !== null
            text: "Installing erases " + (page.disk ? page.disk.path : "") + " completely."
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
        InstallerText {
            tone: "muted"
            text: "Not applied yet: " + [
                InstallerState.ntp ? "automatic clock" : "",
                InstallerState.autoLogin ? "automatic log in" : "",
                InstallerState.rootPassword ? "root password" : ""
            ].filter(Boolean).join(", ")
            visible: InstallerState.ntp || InstallerState.autoLogin || InstallerState.rootPassword
        }
        InstallerSwitchRow {
            visible: page.disk !== null
            label: "I understand that everything on " + (page.disk ? page.disk.path : "the disk") + " will be erased."
            checked: page.confirmedDisk !== "" && page.confirmedDisk === InstallerState.diskPath
            onToggled: value => page.confirmedDisk = value ? InstallerState.diskPath : ""
        }
    }
}
