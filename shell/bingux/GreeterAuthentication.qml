import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd

// SessionScreen authentication contract over greetd. greetd owns the PAM
// conversation and the privileged session start; this object only relays
// prompts and the chosen session command.
QtObject {
    id: root

    property string userName: ""
    // The wayland-sessions entry to start: { id, command, desktopNames }.
    property var session: null
    readonly property bool available: Greetd.available
    readonly property bool active: Greetd.state === GreetdState.Authenticating || Greetd.state === GreetdState.ReadyToLaunch
    readonly property bool finishing: Greetd.state === GreetdState.ReadyToLaunch || Greetd.state === GreetdState.Launching || Greetd.state === GreetdState.Launched
    property bool responseRequired: false
    property bool responseVisible: false
    property string prompt: ""
    property string status: ""
    property bool statusIsError: false
    property string hint: "Enter your password"
    property bool fingerprintAvailable: false
    property bool faceAvailable: false
    signal responseRequested
    signal launchFailed(string message)

    function start() {
        if (!available) {
            status = "The greeter is not connected to greetd";
            statusIsError = true;
            return;
        }
        if (userName === "" || Greetd.state !== GreetdState.Inactive)
            return;
        if (!statusIsError)
            status = "";
        responseRequired = false;
        prompt = "";
        biometrics.running = true;
        Greetd.createSession(userName);
    }

    // After a failure greetd has ended the session. Keep the prompt open and
    // open a fresh conversation with the next response, so the user only has
    // to type the password again. The response is held until the new prompt
    // arrives and is never logged.
    property string pendingResponse: ""

    function respond(value) {
        if (!responseRequired)
            return;
        status = "";
        statusIsError = false;
        if (Greetd.state === GreetdState.Authenticating) {
            responseRequired = false;
            Greetd.respond(value);
            return;
        }
        if (Greetd.state === GreetdState.Inactive && userName !== "") {
            responseRequired = false;
            pendingResponse = value;
            biometrics.running = true;
            Greetd.createSession(userName);
        }
    }

    function cancel() {
        if (Greetd.state === GreetdState.Authenticating)
            Greetd.cancelSession();
        pendingResponse = "";
        responseRequired = false;
        prompt = "";
        status = "";
        statusIsError = false;
    }

    function launchEnvironment() {
        const environment = ["XDG_SESSION_TYPE=wayland"];
        if (session.id)
            environment.push("XDG_SESSION_DESKTOP=" + session.id);
        if (session.desktopNames)
            environment.push("XDG_CURRENT_DESKTOP=" + session.desktopNames);
        return environment;
    }

    function updateHint() {
        if (fingerprintAvailable && faceAvailable)
            hint = "Face, fingerprint or enter your password";
        else if (fingerprintAvailable)
            hint = "Fingerprint or enter your password";
        else if (faceAvailable)
            hint = "Face or enter your password";
        else
            hint = "Enter your password";
    }

    onUserNameChanged: {
        fingerprintAvailable = false;
        faceAvailable = false;
        updateHint();
    }

    property Process biometrics: Process {
        command: ["bingux-session", "biometrics", root.userName, "greetd"]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    root.fingerprintAvailable = result.fingerprint === true;
                    root.faceAvailable = result.face === true;
                } catch (_) {
                    root.fingerprintAvailable = false;
                    root.faceAvailable = false;
                }
                root.updateHint();
            }
        }
    }

    property Process remember: Process {
        command: ["bingux-session", "remember", root.userName, root.session ? root.session.id : ""]
        onExited: Greetd.launch(root.session.command, root.launchEnvironment(), true)
    }

    property Connections greetd: Connections {
        target: Greetd
        function onAuthMessage(message, error, required, echoResponse) {
            root.responseVisible = echoResponse;
            if (required && root.pendingResponse !== "") {
                const response = root.pendingResponse;
                root.pendingResponse = "";
                root.responseRequired = false;
                Greetd.respond(response);
                return;
            }
            root.responseRequired = required;
            if (required) {
                root.prompt = message.replace(/:\s*$/, "");
                root.responseRequested();
            } else {
                root.status = message;
                root.statusIsError = error;
            }
        }
        function onAuthFailure(message) {
            root.pendingResponse = "";
            root.status = message && message !== "pam_authenticate failed" ? message : "Authentication failed";
            root.statusIsError = true;
            // greetd ended the session. Leave the password prompt open; the
            // next response opens a new conversation.
            root.prompt = "Password";
            root.responseRequired = true;
            root.responseRequested();
        }
        function onReadyToLaunch() {
            if (!root.session || !root.session.command || root.session.command.length === 0) {
                root.status = "No session is available to start";
                root.statusIsError = true;
                Greetd.cancelSession();
                return;
            }
            root.status = "Starting " + (root.session.name || root.session.id) + "…";
            root.statusIsError = false;
            root.remember.running = true;
        }
        function onError(message) {
            root.responseRequired = false;
            root.status = message || "greetd reported an error";
            root.statusIsError = true;
            root.launchFailed(root.status);
        }
    }
}
