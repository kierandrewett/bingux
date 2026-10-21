//@ pragma UseQApplication
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd
import Quickshell.Wayland
import "../shell/bingux"

// Drives the greeter against the fake greetd server from greeter-ui.py.
// A shot prints "GREETER_UI_SHOT <name>" and pauses the script. The harness
// grabs the screen and then rewrites the ack file, which resumes the script.
// No timers: every state change happens after the previous screenshot is taken.
// Prints GREETER_UI_PASSED once a wrong password is reported, a right one
// reaches readyToLaunch, and the launch request has been acknowledged.
ShellRoot {
    id: root
    property int step: 0
    property string wrongStatus: ""
    property var pending: []
    property bool paused: false

    function fail(message) {
        console.error("GREETER_UI_FAILED: " + message);
        Qt.exit(1);
    }

    function queue(action) {
        pending.push(action);
        drain();
    }

    function drain() {
        while (pending.length > 0 && !paused) {
            var action = pending.shift();
            action();
        }
    }

    function shot(name) {
        console.error("GREETER_UI_SHOT " + name);
        paused = true;
    }

    Component.onCompleted: SessionContext.enter("greeter")

    // The harness rewrites this file after each screenshot to resume.
    FileView {
        path: Quickshell.env("GREETER_UI_ACK")
        printErrors: false
        watchChanges: true
        onFileChanged: {
            reload();
            root.paused = false;
            root.drain();
        }
    }
    Metrics {
        id: metrics
    }

    GreeterAuthentication {
        id: authentication
        onLaunchFailed: message => root.fail("launch failed: " + message)
    }
    GreeterCatalog {
        id: catalog
    }

    PanelWindow {
        id: window
        color: "transparent"
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
            metrics: metrics
            hostWindow: window
        }
    }

    Timer {
        id: watchdog
        interval: 90000
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
                root.queue(() => root.shot("idle"));
                root.queue(() => { screen.responseField.text = "wro"; });
                root.queue(() => root.shot("typing"));
                root.queue(() => authentication.respond("wrong"));
            } else if (root.step === 1) {
                // The failure keeps the prompt open and shows its text.
                if (!authentication.statusIsError)
                    root.fail("failure was not reported as an error");
                root.wrongStatus = authentication.status;
                if (String(screen.wallpaper) === "")
                    root.fail("the selected account's wallpaper was not applied");
                root.step = 2;
                root.queue(() => root.shot("wrong-password"));
                root.queue(() => screen.pageStack.go("accounts"));
                root.queue(() => root.shot("accounts"));
                root.queue(() => screen.pageStack.back());
                root.queue(() => screen.pageStack.go("sessions"));
                root.queue(() => root.shot("sessions"));
                root.queue(() => screen.pageStack.back());
                root.queue(() => screen.toggleControlCentre());
                root.queue(() => root.shot("control-centre"));
                root.queue(() => screen.toggleControlCentre());
                root.queue(() => screen.focusPrompt());
                root.queue(() => { screen.responseField.text = "secret"; });
                root.queue(() => root.shot("typed-password"));
                root.queue(() => authentication.respond("secret"));
            }
        }
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
