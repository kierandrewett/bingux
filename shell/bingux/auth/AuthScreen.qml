import QtQuick
import QtQuick.Layouts
import ".."

// The layout shared by the standalone lock screen and the bsm greeter.
//
// The wallpaper fills the screen. The date and clock sit near the top, the
// account card (picture, name, status, response field) near the bottom, and
// the host fills slots for everything else: `topContent` (the session bar),
// `overlayContent` (on-screen feedback and panels, above everything),
// `leadingContent` (inside the card, above the picture), `cardContent`
// (inside the card, under the status line), and the two bottom corners.
//
// The item does not know how a user is authenticated. The host sets the card's
// texts and reacts to `submitted(text)` and `retryRequested()`. `entered` and
// `leaving` drive the lock screen's own entrance and exit motion. The bsm
// greeter leaves `leaving` false: bsm runs the outer reveal and unlock.
Item {
    id: root

    property AuthTheme theme: AuthTheme {}

    property bool interactive: true
    property bool entered: false
    property bool leaving: false
    readonly property bool showingControls: entered && !leaving

    property url wallpaper: ""
    property int wallpaperRevision: 0
    property bool cacheWallpaper: false
    // Cross-fade between wallpapers. The lock screen has one picture for its
    // whole life and appears without a fade.
    property bool crossfade: true

    property string displayName: ""
    property url avatarSource: ""
    property bool showAccount: true
    property bool showForm: true
    property string statusText: ""
    property bool statusIsError: false
    property string promptText: ""
    property bool echoVisible: false
    property bool canSubmit: false
    property bool busy: false
    property string submitLabel: "Unlock"
    property bool showRetry: false
    property string retryText: "Try again"
    property bool showClock: true

    property alias topContent: top.data
    property alias overlayContent: overlay.data
    property alias leadingContent: card.leadingContent
    property alias cardContent: card.content
    property alias cornerLeadingContent: cornerLeading.data
    property alias cornerContent: corner.data
    // The wallpaper's picture layer, for frosted surfaces placed by the host.
    readonly property Item backdrop: background.backdrop
    readonly property alias cardItem: card
    readonly property alias clockItem: clock

    signal submitted(string text)
    signal retryRequested

    function focusPrompt() {
        if (interactive)
            card.focusField();
    }

    function clearResponse() {
        card.clear();
    }

    function reject() {
        card.reject();
    }

    Binding {
        target: root.theme
        property: "wallpaperShown"
        value: background.pictureShown
    }

    // A slow settle on entry and a gentle push on exit, so the wallpaper reads
    // as the desktop coming forward rather than a cut.
    BackgroundView {
        id: background
        anchors.fill: parent
        source: root.wallpaper
        revision: root.wallpaperRevision
        cache: root.cacheWallpaper
        tintTop: root.theme.tintTop
        tintBottom: root.theme.tintBottom
        fadeDuration: root.crossfade ? root.theme.fadeMotion : 0
        scrim: !root.leaving
        scrimFadeDuration: root.theme.exitMotion
        zoom: root.leaving ? 1.04 : root.entered ? 1 : 1.03
        Behavior on zoom {
            NumberAnimation {
                duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion * 2
                easing.type: Easing.OutCubic
            }
        }
    }

    Item {
        id: top
        anchors.fill: parent
        z: 20
    }
    Item {
        id: overlay
        anchors.fill: parent
        z: 25
    }

    AuthClock {
        id: clock
        theme: root.theme
        anchors.top: parent.top
        anchors.topMargin: Math.round(root.height * 0.08)
        anchors.horizontalCenter: parent.horizontalCenter
        visible: root.showClock
        opacity: root.showingControls ? 1 : 0
        transform: Translate {
            y: root.leaving ? -16 : root.entered ? 0 : 24
            Behavior on y {
                NumberAnimation {
                    duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion
                    easing.type: Easing.OutCubic
                }
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion
            }
        }
    }

    AuthCard {
        id: card
        theme: root.theme
        backdrop: root.backdrop
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Math.round(root.height * 0.1)
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(root.width - Theme.paddingLarge * 2, 480)
        interactive: root.interactive
        showAccount: root.showAccount
        showForm: root.showForm
        displayName: root.displayName
        avatarSource: root.avatarSource
        statusText: root.statusText
        statusIsError: root.statusIsError
        promptText: root.promptText
        echoVisible: root.echoVisible
        canSubmit: root.canSubmit
        busy: root.busy
        submitLabel: root.submitLabel
        showRetry: root.showRetry
        retryText: root.retryText
        opacity: root.showingControls ? 1 : 0
        // The exit shrinks the identity a little as it fades, like a sheet
        // closing; the entrance rises from just below its resting place.
        scale: root.leaving ? 0.96 : 1
        transform: Translate {
            y: root.entered ? 0 : 28
            Behavior on y {
                NumberAnimation {
                    duration: root.theme.enterMotion
                    easing.type: Easing.OutCubic
                }
            }
        }
        Behavior on opacity {
            NumberAnimation {
                duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion
            }
        }
        Behavior on scale {
            NumberAnimation {
                duration: root.theme.exitMotion
                easing.type: Easing.InCubic
            }
        }
        onSubmitted: text => root.submitted(text)
        onRetryRequested: root.retryRequested()
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
                duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion
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
                duration: root.leaving ? root.theme.exitMotion : root.theme.enterMotion
            }
        }
    }
}
