import QtQuick
import "InstallerSchema.js" as Schema

// Shows the install while the daemon runs. Cancel is offered only before the
// commit point (the partition phase); the daemon refuses it afterwards.
InstallerPage {
    id: page

    title: "Installing Bingux"
    lead: "This takes a few minutes. You can leave this screen open."
    fullBody: true
    allowBack: false
    primaryVisible: false
    bodyHeight: 600

    // InstallerClient, which cancels the install.
    property var client: null
    property bool showLog: false
    property real sweep: 0

    readonly property real us: InstallerState.uiScale
    readonly property bool cancellable: InstallerState.installState === "running"
        && ["", "validate", "preflight"].indexOf(InstallerState.phase) >= 0
    readonly property real fraction: InstallerState.percent >= 0 ? Math.min(1, InstallerState.percent / 100) : -1

    // The daemon's phases, in order. The step list marks each as done, active or pending.
    readonly property var steps: ["validate", "preflight", "partition", "mount", "copy", "strip", "configure", "bootloader", "finish"]
    readonly property int activeIndex: steps.indexOf(InstallerState.phase)

    function stepState(index) {
        if (InstallerState.installState === "done")
            return "done";
        if (activeIndex < 0)
            return "pending";
        if (index < activeIndex)
            return "done";
        return index === activeIndex ? "active" : "pending";
    }

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

    InstallerScroll {
        spacing: Math.round(18 * page.us)

        Row {
            width: parent.width
            spacing: Theme.gap * 2

            // Left: the current step, a large percentage and the bar.
            Column {
                width: Math.round(parent.width * 0.56)
                spacing: Math.round(14 * page.us)

                Text {
                    width: parent.width
                    text: InstallerState.phase === "" ? "Starting..." : Schema.phaseLabel(InstallerState.phase)
                    wrapMode: Text.WordWrap
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(30 * page.us)
                    font.weight: Font.DemiBold
                }
                Text {
                    text: page.fraction >= 0 ? Math.round(page.fraction * 100) + "%" : "Working..."
                    color: Theme.accent
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(56 * page.us)
                    font.weight: Font.Bold
                }
                Rectangle {
                    id: track
                    width: parent.width
                    height: Math.round(22 * page.us)
                    radius: height / 2
                    color: Qt.rgba(1, 1, 1, 0.12)
                    clip: true

                    Rectangle {
                        height: parent.height
                        radius: height / 2
                        color: Theme.accent
                        width: page.fraction >= 0 ? track.width * page.fraction : track.width * 0.3
                        x: page.fraction >= 0 ? 0 : (track.width - width) * page.sweep
                    }
                }
                InstallerText {
                    visible: InstallerState.phaseMessage !== ""
                    text: InstallerState.phaseMessage
                }
                InstallerNotice {
                    visible: InstallerState.installState === "running" && !page.cancellable
                    tone: "warning"
                    title: "The disk is being changed"
                    body: "The installation cannot be cancelled now."
                }
            }

            // Right: the step list.
            Column {
                width: parent.width - Math.round(parent.width * 0.56) - Theme.gap * 2
                spacing: Math.round(2 * page.us)

                Repeater {
                    model: page.steps
                    Item {
                        required property string modelData
                        required property int index
                        readonly property string mode: page.stepState(index)
                        width: parent.width
                        height: Math.round(34 * page.us)

                        Rectangle {
                            id: mark
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.round(26 * page.us)
                            height: width
                            radius: width / 2
                            color: parent.mode === "done" ? Theme.success : "transparent"
                            border.width: 2
                            border.color: parent.mode === "done" ? Theme.success
                                : parent.mode === "active" ? Theme.accent : Qt.rgba(1, 1, 1, 0.3)

                            Rectangle {
                                visible: parent.parent.mode === "active"
                                anchors.centerIn: parent
                                width: parent.width / 2
                                height: width
                                radius: width / 2
                                color: Theme.accent
                            }
                        }
                        Text {
                            anchors.left: mark.right
                            anchors.leftMargin: Theme.gap
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: Schema.phaseLabel(modelData)
                            wrapMode: Text.WordWrap
                            color: parent.mode === "pending" ? Theme.muted : Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Math.round(19 * page.us)
                            font.weight: parent.mode === "active" ? Font.DemiBold : Font.Normal
                        }
                    }
                }
            }
        }

        ActionButton {
            flat: true
            text: page.showLog ? "Hide details" : "Show details"
            onClicked: page.showLog = !page.showLog
        }
        InstallerLog {
            visible: page.showLog
            width: parent.width
            lineHeight: Math.round(260 * page.us)
        }
    }
}
