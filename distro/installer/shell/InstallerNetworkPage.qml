import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// Optional network step. Wi-Fi and wired status come from NetworkManager
// (nmcli). Joining is optional: Continue works without a connection.
InstallerPage {
    id: page

    title: "Network"
    bodyHeight: 420

    property var networks: []
    property string wired: ""
    property string status: ""
    property string selected: ""
    property string password: ""
    property bool busy: false

    readonly property var selectedNetwork: {
        for (const network of networks)
            if (network.ssid === selected)
                return network;
        return null;
    }
    readonly property bool needsPassword: selectedNetwork !== null && selectedNetwork.secured && !selectedNetwork.connected
    readonly property bool canJoin: selectedNetwork !== null && !selectedNetwork.connected && !busy
        && (!needsPassword || password.length >= 8)

    function primary() {
        page.wizard.go("timezone");
    }

    onActiveChanged: {
        if (active)
            refresh();
    }

    function refresh() {
        wiredQuery.running = true;
        wifiQuery.running = true;
    }

    function parseWifi(text) {
        const best = {};
        for (const line of text.trim().split("\n")) {
            const fields = line.split(":");
            if (fields.length < 4)
                continue;
            const ssid = fields.slice(3).join(":");
            if (ssid === "")
                continue;
            const network = {
                ssid: ssid,
                connected: fields[0] === "*",
                signal: parseInt(fields[1]) || 0,
                secured: fields[2] !== "" && fields[2] !== "--"
            };
            if (!best[ssid] || network.signal > best[ssid].signal || network.connected)
                best[ssid] = network;
        }
        const list = Object.values(best);
        list.sort((a, b) => (b.connected - a.connected) || (b.signal - a.signal));
        networks = list.slice(0, 6);
    }

    function join() {
        if (!canJoin)
            return;
        page.busy = true;
        page.status = "Joining " + selected + "...";
        const command = ["nmcli", "--wait", "30", "device", "wifi", "connect", selected];
        if (needsPassword)
            command.push("password", password);
        joinQuery.command = command;
        joinQuery.running = true;
    }

    Process {
        id: wifiQuery
        command: ["nmcli", "--terse", "--escape", "no", "--fields", "IN-USE,SIGNAL,SECURITY,SSID", "device", "wifi", "list", "--rescan", "auto"]
        stdout: StdioCollector {
            onStreamFinished: page.parseWifi(text)
        }
        onExited: exitCode => {
            if (exitCode !== 0)
                page.status = "Wi-Fi is not available on this computer.";
        }
    }
    Process {
        id: wiredQuery
        command: ["nmcli", "--terse", "--fields", "DEVICE,TYPE,STATE", "device"]
        stdout: StdioCollector {
            onStreamFinished: {
                const states = [];
                for (const line of text.trim().split("\n")) {
                    const fields = line.split(":");
                    if (fields.length >= 3 && fields[1] === "ethernet")
                        states.push(fields[2] === "connected" ? "connected" : "not connected");
                }
                page.wired = states.length ? "Wired: " + states[0] : "";
            }
        }
    }
    Process {
        id: joinQuery
        onExited: exitCode => {
            page.busy = false;
            page.password = "";
            if (exitCode === 0)
                page.status = "Connected to " + page.selected + ".";
            else
                page.status = "Could not join " + page.selected + ". Check the password and try again.";
            page.refresh();
        }
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        InstallerText {
            text: "Joining a network is optional. The installer can continue without one."
        }
        InstallerText {
            visible: page.wired !== ""
            tone: "text"
            text: page.wired
        }
        Repeater {
            model: page.networks
            SessionRow {
                required property var modelData
                width: parent.width
                text: modelData.ssid + (modelData.connected ? " (connected)" : "") + (modelData.secured ? " (secured)" : "")
                selected: page.selected === modelData.ssid
                ticked: modelData.connected
                onClicked: {
                    page.selected = modelData.ssid;
                    page.password = "";
                    page.status = "";
                }
            }
        }
        InstallerText {
            visible: page.networks.length === 0 && page.status === ""
            text: "No Wi-Fi networks found yet."
        }
        InstallerField {
            visible: page.needsPassword
            caption: "Password for " + page.selected
            echoMode: TextInput.Password
            text: page.password
            onEdited: page.password = text
            onAccepted: page.join()
        }
        RowLayout {
            width: parent.width
            spacing: Theme.gap
            ActionButton {
                text: "Refresh"
                flat: true
                onClicked: page.refresh()
            }
            Item {
                Layout.fillWidth: true
            }
            ActionButton {
                visible: page.selectedNetwork !== null && !page.selectedNetwork.connected
                enabled: page.canJoin
                text: "Join"
                onClicked: page.join()
            }
        }
        InstallerText {
            visible: page.status !== ""
            tone: page.status.indexOf("Could not") === 0 || page.status.indexOf("not available") >= 0 ? "danger" : "text"
            text: page.status
        }
    }
}
