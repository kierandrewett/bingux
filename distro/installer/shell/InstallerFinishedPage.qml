import QtQuick
import Quickshell
import "InstallerSchema.js" as Schema

// The install succeeded. The daemon never reboots; the user restarts here, or
// closes the installer and reboots later.
InstallerPage {
    id: page

    title: "Bingux is installed"
    lead: "Your computer is ready. Restart to start Bingux from the disk."
    fullBody: true
    allowBack: false
    primaryText: "Restart"
    bodyHeight: 600
    secondaryText: "Keep trying Bingux"
    secondaryVisible: true

    readonly property var result: InstallerState.result || ({})
    readonly property real us: InstallerState.uiScale

    function primary() {
        Quickshell.execDetached(["systemctl", "reboot"]);
    }

    function secondary() {
        Quickshell.execDetached(["bingux-live-switch", "live"]);
    }

    InstallerScroll {
        spacing: Math.round(22 * page.us)

        // The success mark: a filled circle with a drawn check.
        Item {
            id: badge
            width: Math.round(140 * page.us)
            height: width
            anchors.horizontalCenter: parent.horizontalCenter

            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: Theme.success
            }
            Rectangle {
                width: badge.width * 0.22
                height: badge.width * 0.09
                radius: height / 2
                color: Qt.rgba(0.05, 0.07, 0.1, 1)
                rotation: 45
                x: badge.width * 0.27
                y: badge.height * 0.56
            }
            Rectangle {
                width: badge.width * 0.44
                height: badge.width * 0.09
                radius: height / 2
                color: Qt.rgba(0.05, 0.07, 0.1, 1)
                rotation: -45
                x: badge.width * 0.4
                y: badge.height * 0.56
            }
        }

        InstallerNotice {
            tone: "info"
            title: "Before you restart"
            body: "Remove the installation medium, so the computer boots from the disk."
        }

        InstallerText {
            visible: page.result.disk !== undefined
            tone: "text"
            text: "Installed on " + (page.result.disk || "the disk") + " with "
                + (page.result.filesystem || InstallerState.filesystem)
                + (page.result.firmware ? " for " + (page.result.firmware === "uefi" ? "UEFI" : "BIOS") + " firmware" : "") + "."
        }
    }
}
