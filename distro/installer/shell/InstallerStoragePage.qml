import QtQuick
import "InstallerSchema.js" as Schema

// Root filesystem, swap and encryption. Only ext4 and btrfs, no swap and LUKS2
// on UEFI are in the protocol; the other choices are listed but disabled.
InstallerPage {
    id: page

    title: "Storage"
    bodyHeight: 520

    property string passphraseConfirm: ""
    readonly property bool uefi: InstallerState.firmware === "uefi"
    readonly property string passphraseError: InstallerState.encryption ? Schema.validPassphrase(InstallerState.passphrase) : ""
    readonly property string confirmError: InstallerState.encryption && passphraseConfirm !== InstallerState.passphrase
        ? "The passphrases do not match." : ""

    primaryEnabled: !InstallerState.encryption || (passphraseError === "" && confirmError === "")

    function primary() {
        page.wizard.go("account");
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
            text: "ext4 (recommended)"
            selected: InstallerState.filesystem === "ext4"
            ticked: selected
            onClicked: InstallerState.filesystem = "ext4"
        }
        SessionRow {
            width: parent.width
            text: "btrfs (subvolumes for / and /home)"
            selected: InstallerState.filesystem === "btrfs"
            ticked: selected
            onClicked: InstallerState.filesystem = "btrfs"
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "XFS (not yet available)"
        }

        InstallerText {
            tone: "text"
            text: "Swap"
        }
        SessionRow {
            width: parent.width
            text: "No swap"
            selected: true
            ticked: true
        }
        SessionRow {
            width: parent.width
            enabled: false
            text: "Swap partition, swap file or zram (not yet available)"
        }

        InstallerSwitchRow {
            label: "Encrypt the system disk (LUKS2)"
            note: page.uefi ? "You type the passphrase at every start. Bingux cannot recover a lost passphrase."
                : "Encryption needs UEFI firmware; this computer uses BIOS."
            enabled: page.uefi
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
