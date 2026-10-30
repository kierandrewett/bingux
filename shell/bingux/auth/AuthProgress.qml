import QtQuick
import QtQuick.Layouts
import ".."
import "ProgressModel.js" as Progress

// Startup progress of a session that is being launched, drawn from bsm's
// progress model and from nothing else.
//
// The bar shows the model's weighted fraction: reached stages count fully and
// a counted stage (displays, surfaces, first frames) counts `done of total`.
// The fill eases between those confirmed values; nothing here advances it by
// a timer. While the pending stage reports no counts the fill holds still and
// a soft pulse at its leading edge says "working", which is an activity
// treatment, not progress. A failure stops the bar and offers Retry, Back to
// accounts and the power menu.
ColumnLayout {
    id: root

    property AuthTheme theme: AuthTheme {}
    property Item backdrop: null
    // A normalised model (ProgressModel.js `normalise`), or null.
    property var progress: null
    // Only the output that takes input offers the failure actions.
    property bool interactive: true

    readonly property bool failed: progress !== null && progress.failure !== null
    readonly property bool indeterminate: progress !== null && Progress.indeterminate(progress)
    readonly property int percent: progress === null ? 0 : Progress.fraction(progress)
    readonly property string stageLabel: progress === null ? "" : failed ? Progress.failureTitle(progress) : Progress.label(progress)
    readonly property string countLabel: progress === null ? "" : Progress.counts(progress)

    signal retryRequested
    signal backRequested
    signal powerRequested

    spacing: Theme.gap
    visible: progress !== null

    Accessible.role: Accessible.ProgressBar
    Accessible.name: stageLabel
    Accessible.description: failed ? (progress.failure.message) : percent + " percent"

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.gap
        Text {
            textFormat: Text.PlainText
            objectName: "progressLabel"
            Layout.fillWidth: true
            text: root.stageLabel
            color: root.failed ? root.theme.danger : root.theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall + 1
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            textFormat: Text.PlainText
            objectName: "progressCount"
            visible: text !== ""
            text: root.countLabel
            color: root.theme.inkMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }

    Item {
        id: track
        objectName: "progressTrack"
        Layout.fillWidth: true
        Layout.preferredHeight: 6

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: root.theme.darkInk ? Qt.rgba(0, 0, 0, 0.14) : Qt.rgba(1, 1, 1, 0.22)
        }
        Rectangle {
            id: fill
            objectName: "progressFill"
            // Eases between confirmed values. It never moves on its own.
            property real value: root.percent / 100
            height: parent.height
            width: Math.max(root.percent > 0 ? height : 0, parent.width * value)
            radius: height / 2
            color: root.failed ? root.theme.danger : root.theme.ink
            Behavior on value {
                NumberAnimation {
                    duration: root.theme.reduceMotion ? 0 : 280
                    easing.type: Easing.OutCubic
                }
            }
        }
        // Activity pulse at the leading edge of the fill while the pending
        // stage has no counts. It does not change the fill.
        Rectangle {
            id: pulse
            objectName: "progressActivity"
            visible: root.indeterminate
            // Fixed width: a width that depended on x made a binding loop.
            readonly property real span: Math.min(48, track.width)
            x: Math.max(0, Math.min(fill.width, track.width - span))
            width: span
            height: track.height
            radius: height / 2
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: Qt.rgba(root.theme.ink.r, root.theme.ink.g, root.theme.ink.b, 0.55)
                }
                GradientStop {
                    position: 1
                    color: Qt.rgba(root.theme.ink.r, root.theme.ink.g, root.theme.ink.b, 0)
                }
            }
            opacity: 1
            SequentialAnimation on opacity {
                running: root.indeterminate && !root.theme.reduceMotion
                loops: Animation.Infinite
                NumberAnimation {
                    to: 0.25
                    duration: 700
                    easing.type: Easing.InOutSine
                }
                NumberAnimation {
                    to: 1
                    duration: 700
                    easing.type: Easing.InOutSine
                }
            }
        }
    }

    Text {
        textFormat: Text.PlainText
        objectName: "progressFailure"
        Layout.fillWidth: true
        visible: root.failed && text !== ""
        text: root.failed ? root.progress.failure.message : ""
        color: root.theme.inkMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
    }

    RowLayout {
        objectName: "progressFailureActions"
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: Theme.spaceSmall
        visible: root.failed && root.interactive
        spacing: Theme.gap
        AuthButton {
            objectName: "progressRetry"
            theme: root.theme
            backdrop: root.backdrop
            primary: true
            text: "Retry"
            onClicked: root.retryRequested()
        }
        AuthButton {
            objectName: "progressBack"
            theme: root.theme
            backdrop: root.backdrop
            text: "Back to accounts"
            onClicked: root.backRequested()
        }
        AuthButton {
            objectName: "progressPower"
            theme: root.theme
            backdrop: root.backdrop
            iconName: "system-shutdown-symbolic"
            Accessible.name: "Power options"
            onClicked: root.powerRequested()
        }
    }
}
