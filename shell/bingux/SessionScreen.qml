import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

// One screen for the lock client and the login greeter. The caller supplies an
// authentication object with this contract:
//   active, responseRequired, responseVisible, prompt, status, statusIsError,
//   finishing, hint, start(), respond(text), finish(),
//   signal responseRequested()
// `finishing` turns true once the credentials were accepted. The interactive
// screen then plays its exit and calls finish(), which hands over to the
// desktop (unlock) or to the chosen session (greeter). finish() is idempotent.
//
// The session bar sits at the top, the date and clock below it, the account
// identity and prompt at the bottom, over the user's wallpaper. Surfaces are
// translucent white so they read the same on any wallpaper. The host fills
// three slots: leadingContent (above the account), cornerLeadingContent
// (bottom left) and cornerContent (bottom right).
Item {
    id: root

    required property var authentication
    // False until the host can accept credentials safely, for example until
    // the session lock is secure. Prompting starts as soon as this is true.
    property bool ready: true
    // Only one output carries the prompt; the others show the clock.
    property bool interactive: true
    property string accountName: ""
    property string accountDisplayName: ""
    property url accountImage: ""
    property bool showAccount: true
    property string submitLabel: "Unlock"
    // Resolved by the caller: the user's own wallpaper for the lock, the
    // selected account's exported wallpaper for the greeter.
    property url wallpaper: ""
    // The top bar is on by default; metrics (bingux-statusd) feeds its
    // keyboard layout menu and may be null.
    property bool showBar: true
    property var metrics: null
    // An OsdState for volume feedback, and the output this screen covers.
    // With no state the screen ignores the volume keys and shows no OSD, as
    // on the greeter.
    property var osdState: null
    property string outputName: ""
    property string gnoblinCtlPath: "gnoblinctl"
    property alias leadingContent: leading.data
    property alias cornerLeadingContent: cornerLeading.data
    property alias cornerContent: corner.data
    // The wallpaper item, for SessionGlass surfaces placed by the host.
    readonly property alias backdrop: backdrop
    property date now: new Date()
    readonly property bool twelveHourClock: /\bAP?\b|a$/i.test(Qt.locale().timeFormat(Locale.ShortFormat))
    readonly property bool promptOpen: authentication.responseRequired && !authentication.finishing
    readonly property bool canSubmit: ready && interactive && promptOpen
    readonly property string statusText: authentication.status !== "" ? authentication.status : authentication.hint
    readonly property color ink: "#ffffff"
    readonly property color inkMuted: Qt.rgba(1, 1, 1, 0.72)
    readonly property color glass: Qt.rgba(1, 1, 1, 0.16)
    readonly property color glassOutline: Qt.rgba(1, 1, 1, 0.22)
    // Entrance: false on the first frame, true straight after. Exit: the
    // credentials were accepted and the controls are fading out.
    property bool entered: false
    readonly property bool leaving: authentication.finishing
    readonly property bool showingControls: entered && !leaving

    function submitResponse() {
        if (!canSubmit)
            return;
        authentication.respond(response.text);
        response.text = "";
    }

    function startWhenReady() {
        if (ready && interactive && !authentication.active && !authentication.finishing)
            authentication.start();
    }

    function focusPrompt() {
        if (interactive)
            response.forceActiveFocus();
    }

    onReadyChanged: startWhenReady()
    onLeavingChanged: if (leaving && interactive)
        exit.start()

    // The hand-over waits for the exit to play. Every output runs this, so
    // finish() must tolerate repeated calls.
    Timer {
        id: exit
        interval: Theme.sessionExitMotion
        onTriggered: root.authentication.finish()
    }

    // Without a wallpaper (no snapshot for the account, or "Other" on the
    // greeter) the screen shows a deep tint of the system accent instead of
    // plain black, so the glass and shadows still have something to sit on.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.darker(Theme.accent, 1.7)
            }
            GradientStop {
                position: 1
                color: Qt.darker(Theme.accent, 3.6)
            }
        }
    }
    Image {
        id: backdrop
        objectName: "sessionWallpaper"
        anchors.fill: parent
        source: root.wallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Decoded at its natural size: a sourceSize bound to the screen
        // restarted the asynchronous load and left it in Loading.
        cache: false
        // A slow settle on entry and a gentle push on exit, so the wallpaper
        // reads as the desktop coming forward rather than a cut.
        scale: root.leaving ? 1.04 : root.entered ? 1 : 1.03
        Behavior on scale {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion * 2
                easing.type: Easing.OutCubic
            }
        }
    }
    // Keep the clock and the prompt readable on bright wallpapers.
    Rectangle {
        anchors.fill: parent
        visible: backdrop.status === Image.Ready
        opacity: root.leaving ? 0 : 1
        Behavior on opacity {
            NumberAnimation {
                duration: Theme.sessionExitMotion
            }
        }
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.rgba(0, 0, 0, 0.22)
            }
            GradientStop {
                position: 0.5
                color: Qt.rgba(0, 0, 0, 0.04)
            }
            GradientStop {
                position: 1
                color: Qt.rgba(0, 0, 0, 0.42)
            }
        }
    }

    // The locked-down top bar: keyboard layout, status and power. Menus open
    // inside this surface, above everything else on the screen.
    SessionBar {
        id: bar
        objectName: "sessionBar"
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        z: 20
        visible: root.showBar
        interactive: root.interactive
        backdrop: backdrop
        metrics: root.metrics
        gnoblinCtlPath: root.gnoblinCtlPath
        opacity: root.showingControls ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
            }
        }
    }
    Keys.forwardTo: [osd]
    SessionOsd {
        id: osd
        anchors.fill: parent
        z: 25
        osdState: root.osdState
        outputName: root.outputName
    }
    // A click anywhere else closes the bar's menus.
    MouseArea {
        anchors.fill: parent
        z: 19
        visible: bar.openMenu !== ""
        onClicked: {
            bar.closeMenus();
            root.focusPrompt();
        }
    }

    SessionShadow {
        id: clock
        anchors.top: parent.top
        anchors.topMargin: Math.round(root.height * 0.08)
        anchors.horizontalCenter: parent.horizontalCenter
        opacity: root.showingControls ? 1 : 0
        transform: Translate {
            y: root.leaving ? -16 : root.entered ? 0 : 24
            Behavior on y {
                NumberAnimation {
                    duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
                    easing.type: Easing.OutCubic
                }
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
            }
        }
        Column {
            spacing: 2
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Qt.formatDate(root.now, "ddd d MMM")
                color: root.ink
                font.family: Theme.fontFamily
                font.pixelSize: 20
                font.weight: Font.DemiBold
                opacity: 0.9
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Qt.formatTime(root.now, root.twelveHourClock ? "h:mm" : "HH:mm")
                color: root.ink
                font.family: Theme.fontFamily
                font.pixelSize: 120
                font.weight: Font.Light
                font.letterSpacing: -3
                lineHeight: 0.85
            }
        }
    }

    ColumnLayout {
        id: identity
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Math.round(root.height * 0.1)
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(root.width - Theme.paddingLarge * 2, 480)
        spacing: Theme.gap
        opacity: root.showingControls ? 1 : 0
        // The exit shrinks the identity a little as it fades, like a sheet
        // closing; the entrance rises from just below its resting place.
        scale: root.leaving ? 0.96 : 1
        transform: Translate {
            y: root.entered ? 0 : 28
            Behavior on y {
                NumberAnimation {
                    duration: Theme.sessionEnterMotion
                    easing.type: Easing.OutCubic
                }
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
            }
        }
        Behavior on scale {
            NumberAnimation {
                duration: Theme.sessionExitMotion
                easing.type: Easing.InCubic
            }
        }

        Item {
            id: leading
            Layout.fillWidth: true
            implicitHeight: childrenRect.height
            visible: implicitHeight > 0
        }

        SessionShadow {
            Layout.alignment: Qt.AlignHCenter
            visible: root.showAccount
            padding: 32
            blur: 0.9
            offsetY: 6
            colour: Qt.rgba(0, 0, 0, 0.35)
            SessionAvatar {
                size: 72
                source: root.accountImage
                name: root.accountDisplayName
            }
        }
        SessionShadow {
            Layout.fillWidth: true
            visible: root.showAccount
            implicitHeight: accountLabel.implicitHeight
            padding: 16
            blur: 0.5
            offsetY: 1
            colour: Qt.rgba(0, 0, 0, 0.45)
            Text {
                id: accountLabel
                width: parent.width
                text: root.accountDisplayName
                color: root.ink
                font.family: Theme.fontFamily
                font.pixelSize: 16
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
        }
        Text {
            id: status
            objectName: "sessionStatus"
            Layout.fillWidth: true
            Layout.topMargin: -2
            visible: root.interactive
            text: root.statusText
            color: root.authentication.statusIsError ? Theme.danger : root.inkMuted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            // A new message pulses in rather than snapping.
            onTextChanged: statusPulse.restart()
            SequentialAnimation {
                id: statusPulse
                NumberAnimation {
                    target: status
                    property: "opacity"
                    to: 0.4
                    duration: Theme.motion
                }
                NumberAnimation {
                    target: status
                    property: "opacity"
                    to: 1
                    duration: Theme.motion
                }
            }
        }

        TextField {
            id: response
            objectName: "sessionResponse"
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.spaceSmall
            Layout.preferredWidth: 200
            implicitHeight: 32
            visible: root.interactive
            enabled: root.canSubmit
            opacity: root.canSubmit ? 1 : 0.55
            placeholderText: root.promptOpen && root.authentication.prompt !== "" ? root.authentication.prompt : "Password"
            echoMode: root.authentication.responseVisible ? TextInput.Normal : TextInput.Password
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall + 1
            color: root.ink
            placeholderTextColor: root.inkMuted
            selectByMouse: false
            leftPadding: Theme.padding
            rightPadding: 32
            Accessible.name: placeholderText
            background: SessionGlass {
                backdrop: backdrop
                outline: response.activeFocus ? Qt.rgba(1, 1, 1, 0.6) : root.glassOutline
                Behavior on outline {
                    ColorAnimation {
                        duration: Theme.motion
                    }
                }
            }
            onAccepted: root.submitResponse()
            // A wrong password shakes the field, the usual sign that the
            // attempt was rejected without reading the status line.
            Connections {
                target: root.authentication
                function onStatusIsErrorChanged() {
                    if (root.authentication.statusIsError)
                        shake.restart();
                }
            }
            SequentialAnimation {
                id: shake
                loops: 1
                NumberAnimation {
                    target: response
                    property: "Layout.leftMargin"
                    to: -8
                    duration: Theme.motion / 2
                }
                NumberAnimation {
                    target: response
                    property: "Layout.leftMargin"
                    to: 8
                    duration: Theme.motion / 2
                }
                NumberAnimation {
                    target: response
                    property: "Layout.leftMargin"
                    to: -4
                    duration: Theme.motion / 2
                }
                NumberAnimation {
                    target: response
                    property: "Layout.leftMargin"
                    to: 0
                    duration: Theme.motion / 2
                }
            }
            AbstractButton {
                id: submit
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 24
                enabled: root.canSubmit
                visible: response.text.length > 0
                Accessible.name: root.submitLabel
                onClicked: root.submitResponse()
                background: Rectangle {
                    radius: 12
                    color: submit.down ? Qt.rgba(1, 1, 1, 0.45) : Qt.rgba(1, 1, 1, 0.3)
                }
                contentItem: SymbolicIcon {
                    implicitSize: 12
                    source: AdwaitaIcons.source("go-next-symbolic")
                    color: root.ink
                }
            }
        }
        SessionLink {
            objectName: "sessionRetry"
            Layout.alignment: Qt.AlignHCenter
            visible: root.interactive && root.ready && !root.authentication.active && !root.authentication.responseRequired && !root.authentication.finishing
            text: "Try again"
            onClicked: root.authentication.start()
        }
    }

    RowLayout {
        id: cornerLeading
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: Theme.paddingLarge
        spacing: Theme.spaceSmall
        visible: root.interactive
        opacity: root.showingControls ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
            }
        }
    }

    RowLayout {
        id: corner
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Theme.paddingLarge
        spacing: Theme.spaceSmall
        visible: root.interactive
        opacity: root.showingControls ? 1 : 0
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
            }
        }
    }

    SystemClock {
        precision: SystemClock.Minutes
        onDateChanged: root.now = date
    }

    Component.onCompleted: {
        focusPrompt();
        startWhenReady();
        entered = true;
    }
    Connections {
        target: root.authentication
        function onResponseRequested() {
            response.text = "";
            root.focusPrompt();
        }
    }
}
