import QtQuick
import Quickshell.Io
import ".."
import "../auth"

// A read-only network status for the corner of the greeter: wired, Wi-Fi or
// offline, from NetworkManager's own client. The greeter cannot join networks;
// that needs a user session. Nothing is shown when NetworkManager is absent.
SessionGlass {
    id: root

    property AuthTheme theme: AuthTheme {}
    // "wired", "wifi" or "offline"; empty until NetworkManager answered.
    // Only the output that takes input watches the network; the others run no process.
    property bool active: true
    property string state: ""
    readonly property string iconName: state === "wired" ? "network-wired-symbolic" : state === "wifi" ? "network-wireless-symbolic" : "network-offline-symbolic"
    readonly property string description: state === "wired" ? "Network: wired, connected" : state === "wifi" ? "Network: Wi-Fi, connected" : "Network: not connected"

    implicitWidth: 32
    implicitHeight: 32
    visible: active && state !== ""
    tint: theme.glass
    outline: theme.glassOutline
    Accessible.role: Accessible.StaticText
    Accessible.name: description

    SymbolicIcon {
        anchors.centerIn: parent
        implicitSize: Theme.iconSize
        source: BinguxIcons.source(root.iconName)
        color: root.theme.ink
    }

    // `TYPE:STATE` per device, for example `ethernet:connected`.
    function parse(text) {
        let next = "offline";
        for (const line of text.split("\n")) {
            const parts = line.split(":");
            if (parts.length < 2 || parts[1] !== "connected")
                continue;
            if (parts[0] === "ethernet")
                next = "wired";
            else if (parts[0] === "wifi" && next !== "wired")
                next = "wifi";
        }
        root.state = next;
    }

    Process {
        id: query
        command: ["nmcli", "--terse", "--fields", "TYPE,STATE", "device", "status"]
        running: root.active
        stdout: StdioCollector {
            onStreamFinished: root.parse(text)
        }
    }
    // `nmcli monitor` prints a line for every change; each one re-asks.
    Process {
        command: ["nmcli", "monitor"]
        running: root.active
        stdout: SplitParser {
            onRead: _line => {
                if (!query.running)
                    query.running = true;
            }
        }
    }
}
