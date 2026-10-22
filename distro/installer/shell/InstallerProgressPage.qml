import QtQuick
import "InstallerSchema.js" as Schema

// Shows the install while the daemon runs. Cancel is offered only before the
// commit point (the partition phase); the daemon refuses it afterwards.
InstallerPage {
    id: page

    title: "Installing Bingux"
    allowBack: false
    primaryVisible: false
    bodyHeight: 420

    // InstallerClient, which cancels the install.
    property var client: null
    property bool showLog: false
    property real sweep: 0

    readonly property bool cancellable: InstallerState.installState === "running"
        && ["", "validate", "preflight"].indexOf(InstallerState.phase) >= 0
    readonly property real fraction: InstallerState.percent >= 0 ? Math.min(1, InstallerState.percent / 100) : -1

    secondaryText: "Cancel"
    secondaryVisible: cancellable

    function secondary() {
        if (cancellable)
            page.client.cancel();
    }

    SequentialAnimation {
        id: sweeping
        running: page.fraction < 0 && !Theme.reducedMotion && InstallerState.installState === "running"
        loops: Animation.Infinite
        NumberAnimation {
            target: page
            property: "sweep"
            from: 0
            to: 1
            duration: 1400
            easing.type: Easing.InOutQuad
        }
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            tone: "text"
            text: InstallerState.phase === "" ? "Starting..." : Schema.phaseLabel(InstallerState.phase)
        }
        Rectangle {
            id: track
            width: parent.width
            height: 8
            radius: 4
            color: Qt.rgba(1, 1, 1, 0.12)
            clip: true

            Rectangle {
                height: parent.height
                radius: 4
                color: Theme.accent
                width: page.fraction >= 0 ? track.width * page.fraction : track.width * 0.3
                x: page.fraction >= 0 ? 0 : (track.width - width) * page.sweep
            }
        }
        InstallerText {
            text: page.fraction >= 0 ? Math.round(page.fraction * 100) + "%" : ""
        }
        InstallerText {
            visible: InstallerState.phaseMessage !== ""
            text: InstallerState.phaseMessage
        }
        InstallerText {
            visible: InstallerState.installState === "running" && !page.cancellable
            tone: "warning"
            text: "The disk is being changed. The installation cannot be cancelled now."
        }
        ActionButton {
            flat: true
            text: page.showLog ? "Hide details" : "Show details"
            onClicked: page.showLog = !page.showLog
        }
        InstallerLog {
            visible: page.showLog
            width: parent.width
            lineHeight: 180
        }
    }
}
