import QtQuick

// State is advisory. No popout waits for another UI process to handle input.
ShortcutSession {
    id: root
    property string sessionName: ""
    property var state: ({})
    property var states: ({})
    signal commandReceived(var command)
    function command(name, command) { send({op: "ui-session", action: "command", name, command}); }
    function publish() {
        if (sessionName && capabilities.includes("ui-sessions"))
            send({op: "ui-session", action: "state", name: sessionName, state});
    }
    onStateChanged: publish()
    onCapabilitiesChanged: {
        states = {};
        if (capabilities.includes("ui-sessions")) {
            send({op: "ui-session", action: "watch"});
            publish();
        }
    }
    onUiState: (name, value) => {
        const next = Object.assign({}, states);
        if (value === null) delete next[name]; else next[name] = value;
        states = next;
    }
    onUiCommand: (name, value) => { if (name === sessionName) commandReceived(value); }
}
