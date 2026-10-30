import QtQuick
import Quickshell
import "auth"

// One screen for the lock client and the login greeter. The caller supplies an
// authentication object with this contract:
//   active, responseRequired, responseVisible, prompt, status, statusIsError,
//   finishing, hint, start(), respond(text), finish(),
//   signal responseRequested()
// `finishing` turns true once the credentials were accepted. The interactive
// screen then plays its exit and calls finish(), which hands over to the
// desktop (unlock) or to the chosen session (greeter). finish() is idempotent.
//
// The layout and the visuals are shared with the bsm greeter and live in
// shell/bingux/auth (AuthScreen, AuthCard, BackgroundView, ...). This item
// adapts the authentication contract above to them and adds what only the
// standalone lock has: the session bar, the volume OSD and the exit timer.
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
    // The window the bar's popups anchor to (the lock surface on the lock screen).
    property var hostWindow: null
    // An OsdState for volume feedback, and the output this screen covers.
    // With no state the screen ignores the volume keys and shows no OSD, as
    // on the greeter.
    property var osdState: null
    property string outputName: ""
    property string gnoblinCtlPath: "gnoblinctl"
    property alias leadingContent: screen.leadingContent
    property alias cornerLeadingContent: screen.cornerLeadingContent
    property alias cornerContent: screen.cornerContent
    // The wallpaper layer, for SessionGlass surfaces placed by the host.
    readonly property Item backdrop: screen.backdrop
    readonly property bool promptOpen: authentication.responseRequired && !authentication.finishing
    readonly property bool canSubmit: ready && interactive && promptOpen
    readonly property string statusText: authentication.status !== "" ? authentication.status : authentication.hint
    // Entrance: false on the first frame, true straight after. Exit: the
    // credentials were accepted and the controls are fading out.
    property bool entered: false
    readonly property bool leaving: authentication.finishing
    readonly property bool showingControls: entered && !leaving

    function submitResponse(text) {
        if (!canSubmit)
            return;
        authentication.respond(text);
    }

    function startWhenReady() {
        if (ready && interactive && !authentication.active && !authentication.finishing)
            authentication.start();
    }

    function focusPrompt() {
        screen.focusPrompt();
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

    Keys.forwardTo: [osd]

    AuthTheme {
        id: lockTheme
    }

    AuthScreen {
        id: screen
        anchors.fill: parent
        theme: lockTheme
        interactive: root.interactive
        entered: root.entered
        leaving: root.leaving
        wallpaper: root.wallpaper
        crossfade: false
        displayName: root.accountDisplayName
        avatarSource: root.accountImage
        showAccount: root.showAccount
        statusText: root.statusText
        statusIsError: root.authentication.statusIsError
        promptText: root.promptOpen && root.authentication.prompt !== "" ? root.authentication.prompt : "Password"
        echoVisible: root.authentication.responseVisible
        canSubmit: root.canSubmit
        submitLabel: root.submitLabel
        showRetry: root.ready && !root.authentication.active && !root.authentication.responseRequired && !root.authentication.finishing
        onSubmitted: text => root.submitResponse(text)
        onRetryRequested: root.authentication.start()

        // The locked-down top bar is the desktop StatusBar in the locked context:
        // DesktopLayout and SessionServices decide which widgets appear. Popups the
        // desktop opens from it have no host window here, so they stay closed.
        topContent: StatusBar {
            id: bar
            objectName: "sessionBar"
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            visible: root.showBar
            metrics: root.metrics
            hostWindow: root.hostWindow
            opacity: root.showingControls ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: root.leaving ? Theme.sessionExitMotion : Theme.sessionEnterMotion
                }
            }
        }
        overlayContent: SessionOsd {
            id: osd
            anchors.fill: parent
            osdState: root.osdState
            outputName: root.outputName
        }
    }

    Component.onCompleted: {
        focusPrompt();
        startWhenReady();
        entered = true;
    }
    Connections {
        target: root.authentication
        function onResponseRequested() {
            screen.clearResponse();
            root.focusPrompt();
        }
        function onStatusIsErrorChanged() {
            if (root.authentication.statusIsError)
                screen.reject();
        }
    }
}
