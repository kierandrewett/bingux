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
    secondaryText: "Close installer"
    secondaryVisible: true

    readonly property var result: InstallerState.result || ({})

    function primary() {
        Quickshell.execDetached(["systemctl", "reboot"]);
    }

    function secondary() {
        Qt.quit();
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
        InstallerText {
            visible: InstallerState.ntp || InstallerState.autoLogin || InstallerState.rootPassword
            text: "Some choices are not applied yet: " + [
                InstallerState.ntp ? "automatic clock" : "",
                InstallerState.autoLogin ? "automatic log in" : "",
                InstallerState.rootPassword ? "root password" : ""
            ].filter(Boolean).join(", ") + "."
        }
    }
}
