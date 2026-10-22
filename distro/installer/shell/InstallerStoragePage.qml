import QtQuick
import "InstallerSchema.js" as Schema

// Root filesystem, swap and encryption. Each choice is enabled only when the
// probe's capabilities list it; the others are shown with the reason.
InstallerPage {
    id: page

    title: "Storage"
    bodyHeight: 600

    property string passphraseConfirm: ""
    readonly property bool bios: InstallerState.firmware === "bios"
    readonly property bool encryptionOffered: InstallerState.encryptionOffered()
    readonly property string passphraseError: InstallerState.encryption ? Schema.validPassphrase(InstallerState.passphrase) : ""
    readonly property string confirmError: InstallerState.encryption && passphraseConfirm !== InstallerState.passphrase
        ? "The passphrases do not match." : ""

    primaryEnabled: !InstallerState.encryption || (passphraseError === "" && confirmError === "")

    function primary() {
        page.wizard.go("account");
    }

    // Label of a filesystem row: the name, or why it is disabled.
    function fsLabel(name, text) {
        return InstallerState.offers("filesystems", name) ? text : text + " (not in this installer build yet)";
    }

    function swapLabel(name, text) {
        return InstallerState.offers("swap", name) ? text : text + " (not in this installer build yet)";
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            tone: "text"
            text: "Root file system"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("filesystems", "ext4")
            text: page.fsLabel("ext4", "ext4 (recommended)")
            selected: InstallerState.filesystem === "ext4"
            ticked: selected
            onClicked: InstallerState.filesystem = "ext4"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("filesystems", "btrfs")
            text: page.fsLabel("btrfs", "btrfs (subvolumes for / and /home)")
            selected: InstallerState.filesystem === "btrfs"
            ticked: selected
            onClicked: InstallerState.filesystem = "btrfs"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("filesystems", "xfs")
            text: page.fsLabel("xfs", "XFS (cannot be shrunk later)")
            selected: InstallerState.filesystem === "xfs"
            ticked: selected
            onClicked: InstallerState.filesystem = "xfs"
        }

        InstallerText {
            tone: "text"
            text: "Swap"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("swap", "none")
            text: "No swap"
            selected: InstallerState.swap === "none"
            ticked: selected
            onClicked: InstallerState.swap = "none"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("swap", "zram")
            text: page.swapLabel("zram", "zram (compressed swap in RAM)")
            selected: InstallerState.swap === "zram"
            ticked: selected
            onClicked: InstallerState.swap = "zram"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("swap", "file")
            text: page.swapLabel("file", "Swap file on the root file system")
            selected: InstallerState.swap === "file"
            ticked: selected
            onClicked: InstallerState.swap = "file"
        }
        SessionRow {
            width: parent.width
            enabled: InstallerState.offers("swap", "partition")
            text: page.swapLabel("partition", "Swap partition")
            selected: InstallerState.swap === "partition"
            ticked: selected
            onClicked: InstallerState.swap = "partition"
        }
        Row {
            visible: InstallerState.swap === "file" || InstallerState.swap === "partition"
            width: parent.width
            spacing: Theme.gap
            Repeater {
                model: [2, 4, 8, 16]
                ActionButton {
                    required property int modelData
                    text: modelData + " GiB"
                    flat: InstallerState.swapGiB !== modelData
                    onClicked: InstallerState.swapGiB = modelData
                }
            }
        }

        InstallerSwitchRow {
            label: "Encrypt the system disk (LUKS2)"
            note: !InstallerState.firmware ? "Checking the firmware."
                : !page.encryptionOffered ? "Encryption is not in this installer build for " + (page.bios ? "BIOS" : "UEFI") + " firmware yet."
                : page.bios ? "You type the passphrase at every start. /boot stays unencrypted, so only the root is encrypted."
                : "You type the passphrase at every start. Bingux cannot recover a lost passphrase."
            enabled: page.encryptionOffered
            checked: InstallerState.encryption
            onToggled: value => InstallerState.encryption = value
        }
        InstallerField {
            visible: InstallerState.encryption
            caption: "Passphrase"
            echoMode: TextInput.Password
            text: InstallerState.passphrase
            invalid: page.passphraseError !== "" && InstallerState.passphrase !== ""
            message: page.passphraseError === "" || InstallerState.passphrase === "" ? "" : page.passphraseError
            onEdited: InstallerState.passphrase = text
        }
        InstallerField {
            visible: InstallerState.encryption
            caption: "Confirm passphrase"
            echoMode: TextInput.Password
            text: page.passphraseConfirm
            invalid: page.confirmError !== "" && page.passphraseConfirm !== ""
            message: page.passphraseConfirm === "" ? "" : page.confirmError
            onEdited: page.passphraseConfirm = text
            onAccepted: page.primary()
        }
    }
}
