import QtQuick
import Quickshell
import "InstallerSchema.js" as Schema

// The install succeeded. The daemon never reboots; the user restarts here, or
// closes the installer and reboots later.
InstallerPage {
    id: page

    title: "Bingux is installed"
    allowBack: false
    primaryText: "Restart"
    bodyHeight: 340
    secondaryText: "Keep trying Bingux"
    secondaryVisible: true

    readonly property var result: InstallerState.result || ({})

    function primary() {
        Quickshell.execDetached(["systemctl", "reboot"]);
    }

    function secondary() {
        Quickshell.execDetached(["bingux-live-switch", "live"]);
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            tone: "text"
            text: "Remove the installation medium before you restart, so the computer boots from the disk."
        }
        InstallerText {
            visible: page.result.disk !== undefined
            text: "Installed on " + (page.result.disk || "the disk") + " with "
                + (page.result.filesystem || InstallerState.filesystem)
                + (page.result.firmware ? " for " + (page.result.firmware === "uefi" ? "UEFI" : "BIOS") + " firmware" : "") + "."
        }
    }
}
