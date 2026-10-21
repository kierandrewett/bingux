//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Services.Greetd
import Quickshell.Wayland
import "../shell/bingux"

// Drives the greeter against the fake greetd server from greeter-ui.py.
// Prints GREETER_UI_PASSED once a wrong password is reported, a right one
// reaches readyToLaunch, and the launch request has been acknowledged.
ShellRoot {
    id: root
    property int step: 0
    property string wrongStatus: ""

    function fail(message) {
        console.error("GREETER_UI_FAILED: " + message);
        Qt.exit(1);
    }

    Component.onCompleted: SessionContext.enter("greeter")

    GreeterAuthentication {
        id: authentication
        onLaunchFailed: message => root.fail("launch failed: " + message)
    }
    GreeterCatalog {
        id: catalog
    }

    PanelWindow {
        id: window
        color: Theme.background
        WlrLayershell.namespace: "bingux-greeter"
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        GreeterScreen {
            id: screen
            anchors.fill: parent
            authentication: authentication
            catalog: catalog
        }
    }

    Timer {
        id: watchdog
        interval: 15000
        running: true
        onTriggered: root.fail("timed out at step " + root.step + " state " + GreetdState.toString(Greetd.state) + " status '" + authentication.status + "'")
    }

    Connections {
        target: authentication
        function onResponseRequested() {
            if (!authentication.responseRequired)
                return;
            if (root.step === 0) {
                root.step = 1;
                if (screen.selectedUser !== "luna")
                    root.fail("remembered user was not selected: " + screen.selectedUser);
                if (!screen.selectedSession || screen.selectedSession.id !== "gnoblin")
                    root.fail("gnoblin session was not preferred");
                if (authentication.prompt !== "Password")
                    root.fail("prompt was '" + authentication.prompt + "'");
                authentication.respond("wrong");
            } else if (root.step === 1) {
                // The failure keeps the prompt open and shows its text.
                if (!authentication.statusIsError)
                    root.fail("failure was not reported as an error");
                root.wrongStatus = authentication.status;
                if (String(screen.wallpaper) === "")
                    root.fail("the selected account's wallpaper was not applied");
                root.step = 2;
                shot.running = true;
            }
        }
    }

    // Give the harness a moment to screenshot the error state, then log in.
    Timer {
        id: shot
        interval: 1500
        onTriggered: authentication.respond("secret")
    }

    Connections {
        target: Greetd
        function onLaunched() {
            if (root.wrongStatus === "")
                root.fail("no failure status was recorded");
            console.error("GREETER_UI_PASSED");
            Qt.quit();
        }
    }
}
