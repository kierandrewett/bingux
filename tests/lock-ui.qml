//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Wayland
import Bingux.Wayland
import "../shell/bingux"

// Drives SessionScreen inside a real ext-session-lock surface, with a stand-in
// for LockAuthentication that follows the same contract. The lock must become
// secure, a wrong password must keep the prompt open, and the right one must
// run the exit before finish() sends unlock_and_destroy. Prints
// LOCK_UI_PASSED once the unlock round trip completes.
//
// Between the rejected and the accepted password the harness presses the
// volume, mute and microphone keys on a virtual keyboard (LOCK_UI_AWAIT_KEYS)
// and then publishes a statusd OSD record on osd-v2.sock. The keys must reach
// the lock screen and change the audio stand-ins, and the card must show the
// record (LOCK_UI_OSD_SHOWN).
ShellRoot {
    id: root
    property int step: 0
    property real unlockAt: 0
    // Set by the lock surface's SessionScreen: its id is not visible from here.
    property var lockScreen: null

    // Stand-ins for the default output and input. PipeWire is not running in
    // the headless session.
    QtObject {
        id: sinkStub
        property bool ready: true
        property QtObject audio: QtObject {
            property real volume: 0.5
            property bool muted: true
        }
    }
    QtObject {
        id: sourceStub
        property bool ready: true
        property QtObject audio: QtObject {
            property real volume: 1
            property bool muted: false
        }
    }
    // The keyboard layout menu reads these fields from statusd metrics. Two
    // layouts, the first active, so the popup has a choice to show.
    QtObject {
        id: lockMetrics
        readonly property bool desktopStateAvailable: true
        readonly property string inputSourceLabel: "US"
        readonly property var inputSources: [
            { type: "xkb", id: "us", shortName: "US", displayName: "English (US)" },
            { type: "xkb", id: "de", shortName: "DE", displayName: "German" }
        ]
        readonly property var currentInputSource: inputSources[0]
        readonly property bool screenSharing: false
        readonly property bool microphoneInUse: false
        readonly property bool locationInUse: false
        readonly property bool privacyAvailable: true
        readonly property bool screenSharingAvailable: true
        readonly property bool microphoneAvailable: true
        readonly property bool locationAvailable: true
    }
    OsdState {
        id: lockOsd
    }

    function fail(message) {
        console.error("LOCK_UI_FAILED: " + message);
        Qt.exit(1);
    }

    QtObject {
        id: lockAuthentication
        property var sessionLock: lock
        property bool unlockRequested: false
        readonly property bool finishing: unlockRequested
        property bool finished: false
        property bool active: false
        property bool responseRequired: false
        property bool responseVisible: false
        property string prompt: ""
        property string status: ""
        property bool statusIsError: false
        property string hint: "Enter your password to unlock"
        signal responseRequested
        signal unlockSubmitted
        function start() {
            if (!sessionLock.secure)
                root.fail("start() before the lock was secure");
            active = true;
            responseRequired = true;
            prompt = "Password";
            responseRequested();
        }
        function respond(value) {
            responseRequired = false;
            if (value === "secret") {
                unlockRequested = true;
                root.unlockAt = Date.now();
                status = "Unlocking…";
                statusIsError = false;
                return;
            }
            status = "Authentication failed";
            statusIsError = true;
            responseRequired = true;
            responseRequested();
        }
        function finish() {
            if (!unlockRequested || finished)
                return;
            finished = true;
            if (Date.now() - root.unlockAt < Theme.sessionExitMotion)
                root.fail("finish() ran before the exit motion ended");
            sessionLock.locked = false;
            unlockSubmitted();
        }
    }

    WaylandRoundtrip {
        id: unlockRoundtrip
        onCompleted: {
            if (!lockAuthentication.finished)
                root.fail("round trip before finish()");
            if (lock.locked)
                root.fail("still locked after finish()");
            console.error("LOCK_UI_PASSED");
            Qt.quit();
        }
        onFailed: message => root.fail("round trip failed: " + message)
    }
    Connections {
        target: lockAuthentication
        function onUnlockSubmitted() {
            unlockRoundtrip.synchronize();
        }
        function onResponseRequested() {
            if (root.step === 0) {
                root.step = 1;
                lockAuthentication.respond("wrong");
            } else if (root.step === 1) {
                if (!lockAuthentication.statusIsError)
                    root.fail("the wrong password was not reported");
                root.step = 2;
                const osd = findChildOf(root.lockScreen, "sessionOsd");
                osd.sink = sinkStub;
                osd.source = sourceStub;
                console.error("LOCK_UI_AWAIT_KEYS");
            }
        }
    }

    // The harness publishes the OSD record after the keys. By then the three
    // key presses have been handled, so the stand-ins show their effect.
    Connections {
        target: lockOsd
        function onRequestsChanged() {
            if (root.step !== 2)
                return;
            root.step = 3;
            if (Math.abs(sinkStub.audio.volume - 0.55) > 0.001 || sinkStub.audio.muted !== true)
                root.fail("volume keys: volume " + sinkStub.audio.volume + " muted " + sinkStub.audio.muted + ", expected 0.55 and muted after Up then Mute");
            if (sourceStub.audio.muted !== true)
                root.fail("the microphone key did not mute the input");
            osdCheck.start();
        }
    }
    Timer {
        id: osdCheck
        interval: 400
        onTriggered: {
            const card = findChildOf(root.lockScreen, "sessionOsd").card;
            if (card.opacity !== 1 || !card.visible)
                root.fail("the OSD card is not shown on the lock screen");
            if (findChildOf(card, "osdTitle").text !== "Volume" || card.percentLabel !== "55%")
                root.fail("the OSD card shows " + findChildOf(card, "osdTitle").text + " " + card.percentLabel);
            console.error("LOCK_UI_OSD_SHOWN");
            popupCheck.start();
        }
    }
    // The keyboard layout menu, opened from the lock bar, must map on the lock
    // surface. The popup is a non-visual ShellPopup Scope, so it is reached
    // through its visual selector: menuOpen is the popup's visibility.
    Timer {
        id: popupCheck
        onTriggered: {
            const owner = selectorOnLockBar();
            owner.openMenu();
            popupVerify.start();
        }
    }
    Timer {
        id: popupVerify
        interval: 400
        onTriggered: {
            const owner = selectorOnLockBar();
            if (!owner.menuOpen)
                root.fail("the keyboard layout popup did not open on the lock screen");
            if (!findChildOf(root.lockScreen.hostWindow.contentItem, "keyboardLayoutMenu"))
                root.fail("the keyboard layout menu did not draw into the lock surface");
            console.error("LOCK_UI_POPUP_OPEN");
            shot.start();
        }
    }
    function selectorOnLockBar() {
        let owner = findChildOf(root.lockScreen, "keyboardLayoutLabel");
        if (!owner)
            root.fail("the keyboard layout selector is not on the lock bar");
        while (owner && typeof owner.openMenu !== "function")
            owner = owner.parent;
        if (!owner)
            root.fail("the keyboard layout label has no selector to open it");
        return owner;
    }
    function findChildOf(item, name) {
        for (const child of item.children) {
            if (child.objectName === name)
                return child;
            const found = findChildOf(child, name);
            if (found)
                return found;
        }
        return null;
    }

    // Give the harness a moment to screenshot the locked state, then unlock.
    Timer {
        id: shot
        interval: 1500
        onTriggered: lockAuthentication.respond("secret")
    }
    Timer {
        interval: 25000
        running: true
        onTriggered: root.fail("timed out at step " + root.step + " secure=" + lock.secure)
    }

    Component.onCompleted: {
        SessionContext.enter("locked");
        lock.locked = true;
    }

    WlSessionLock {
        id: lock
        WlSessionLockSurface {
            id: lockSurface
            color: Theme.background
            SessionScreen {
                id: screen
                Component.onCompleted: root.lockScreen = screen
                anchors.fill: parent
                authentication: lockAuthentication
                metrics: lockMetrics
                hostWindow: lockSurface
                ready: lock.secure
                osdState: lockOsd
                outputName: Quickshell.screens[0].name
                accountName: "luna"
                accountDisplayName: "Luna Lovegood"
                wallpaper: Quickshell.env("BINGUX_LOCK_WALLPAPER") !== "" ? "file://" + Quickshell.env("BINGUX_LOCK_WALLPAPER") : ""
            }
        }
    }
}
