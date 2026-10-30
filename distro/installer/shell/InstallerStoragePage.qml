import QtQuick
import "InstallerSchema.js" as Schema

// Root filesystem, swap and encryption, one column each. Each choice is enabled
// only when the probe's capabilities list it; the others say why in their detail line.
InstallerPage {
    id: page

    title: "Storage"
    fullBody: true
    bodyHeight: 600

    property string passphraseConfirm: ""
    readonly property real us: InstallerState.uiScale
    readonly property bool bios: InstallerState.firmware === "bios"
    readonly property bool encryptionOffered: InstallerState.encryptionOffered()
    readonly property bool homeOnOwnPartition: InstallerState.storageMode === "manual" && InstallerState.manualHasRole("home")
    readonly property string passphraseError: InstallerState.encryption ? Schema.validPassphrase(InstallerState.passphrase) : ""
    readonly property string confirmError: InstallerState.encryption && passphraseConfirm !== InstallerState.passphrase
        ? "The passphrases do not match." : ""

    primaryEnabled: !InstallerState.encryption || (passphraseError === "" && confirmError === "")

    function primary() {
        page.wizard.go("account");
    }

    // Detail line of a filesystem row: what it does, or that this build cannot do it yet.
    function fsDetail(name, text) {
        return InstallerState.offers("filesystems", name) ? text : "Not in this installer build yet";
    }

    function swapDetail(name, text) {
        return InstallerState.offers("swap", name) ? text : "Not in this installer build yet";
    }

    InstallerScroll {
        spacing: Math.round(14 * page.us)

        Row {
            width: parent.width
            spacing: Theme.gap * 2

            // Root file system.
            Column {
                width: (parent.width - parent.spacing * 2) / 3
                spacing: Math.round(10 * page.us)

                InstallerText {
                    tone: "text"
                    text: "Root file system"
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("filesystems", "ext4")
                    opacity: enabled ? 1 : 0.5
                    text: "ext4 (recommended)"
                    detail: page.fsDetail("ext4", "Simple and well tested")
                    selected: InstallerState.filesystem === "ext4"
                    ticked: selected
                    onClicked: InstallerState.filesystem = "ext4"
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("filesystems", "btrfs")
                    opacity: enabled ? 1 : 0.5
                    text: "btrfs"
                    detail: page.fsDetail("btrfs", "Subvolumes for / and /home")
                    selected: InstallerState.filesystem === "btrfs"
                    ticked: selected
                    onClicked: InstallerState.filesystem = "btrfs"
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("filesystems", "xfs")
                    opacity: enabled ? 1 : 0.5
                    text: "XFS"
                    detail: page.fsDetail("xfs", "Cannot be shrunk later")
                    selected: InstallerState.filesystem === "xfs"
                    ticked: selected
                    onClicked: InstallerState.filesystem = "xfs"
                }
            }

            // Swap.
            Column {
                width: (parent.width - parent.spacing * 2) / 3
                spacing: Math.round(10 * page.us)

                InstallerText {
                    tone: "text"
                    text: "Swap"
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("swap", "none")
                    opacity: enabled ? 1 : 0.5
                    text: "No swap"
                    detail: "Nothing is reserved"
                    selected: InstallerState.effectiveSwap() === "none"
                    ticked: selected
                    onClicked: InstallerState.setSwap("none")
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("swap", "zram")
                    opacity: enabled ? 1 : 0.5
                    text: "zram"
                    detail: page.swapDetail("zram", "Compressed swap in RAM")
                    selected: InstallerState.effectiveSwap() === "zram"
                    ticked: selected
                    onClicked: InstallerState.setSwap("zram")
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.offers("swap", "file")
                    opacity: enabled ? 1 : 0.5
                    text: "Swap file"
                    detail: page.swapDetail("file", "A file on the root file system")
                    selected: InstallerState.effectiveSwap() === "file"
                    ticked: selected
                    onClicked: InstallerState.setSwap("file")
                }
                InstallerOption {
                    width: parent.width
                    enabled: InstallerState.storageMode !== "manual" && InstallerState.offers("swap", "partition")
                    opacity: enabled ? 1 : 0.5
                    text: "Swap partition"
                    detail: InstallerState.storageMode === "manual"
                        ? "Set on the Partitions page"
                        : page.swapDetail("partition", "Its own partition on the disk")
                    selected: InstallerState.effectiveSwap() === "partition"
                    ticked: selected
                    onClicked: InstallerState.setSwap("partition")
                }
                InstallerText {
                    visible: InstallerState.storageMode !== "manual" && (InstallerState.swap === "file" || InstallerState.swap === "partition")
                    tone: "text"
                    text: "Swap size"
                }
                Flow {
                    visible: InstallerState.storageMode !== "manual" && (InstallerState.swap === "file" || InstallerState.swap === "partition")
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
            }

            // Encryption.
            Column {
                width: parent.width - 2 * ((parent.width - parent.spacing * 2) / 3) - parent.spacing * 2
                spacing: Math.round(10 * page.us)

                InstallerText {
                    tone: "text"
                    text: "Encryption"
                }
                InstallerSwitchRow {
                    label: "Encrypt the system disk (LUKS2)"
                    note: !InstallerState.firmware ? "Checking the firmware."
                        : !page.encryptionOffered ? "Encryption is not in this installer build for " + (page.bios ? "BIOS" : "UEFI") + " firmware yet."
                        : page.homeOnOwnPartition && !InstallerState.encryption ? "A separate home partition on the Partitions page cannot be encrypted. Remove it to encrypt the system."
                        : page.bios ? "You type the passphrase at every start. /boot stays unencrypted, so only the root is encrypted."
                        : "You type the passphrase at every start. Bingux cannot recover a lost passphrase."
                    enabled: page.encryptionOffered && (InstallerState.encryption || !page.homeOnOwnPartition)
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
    }
}
