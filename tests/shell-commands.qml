import QtQuick
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Io
import "../shell/bingux"

ShellRoot {
    id: test
    property int failures: 0
    function check(value, message) { if (!value) { failures++; console.error(message); } }
    QtObject { id: output; property real volume: 0.5; property bool muted: false }
    QtObject { id: input; property real volume: 0.25; property bool muted: true }
    QtObject { id: sink; property bool ready: true; property var audio: output }
    QtObject { id: source; property bool ready: true; property var audio: input }
    QtObject { id: indicatorsFixture; property var audioSink: sink; property var audioSource: source }
    QtObject {
        id: player
        property string dbusName: "org.mpris.MediaPlayer2.test"
        property string identity: "Test player"
        property bool isPlaying: false
        property string trackTitle: "Track"
        property string trackArtist: "Artist"
        property bool positionSupported: true
        property bool lengthSupported: true
        property real position: 0
        property real length: 120
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canGoNext: false
        property bool canGoPrevious: true
        property bool canSeek: true
        function play() { isPlaying = true; }
        function pause() { isPlaying = false; }
        function previous() {}
    }
    QtObject {
        id: servicesFixture
        property bool ready: true
        property bool busy: false
        property string error: ""
        property bool keepAwake: false
        property var lastAction: null
        property var state: ({power: {available: true, profile: "balanced", profiles: ["balanced", "power-saver"]},
            awakeAvailable: true, nightLightAvailable: true, nightLight: false, nightLightActive: false})
        function action(request) { lastAction = request; }
        function toggleAwake() { keepAwake = !keepAwake; }
    }
    QtObject {
        id: devicesFixture
        property var audioOutputs: [{name: "speaker", description: "Speakers"}]
        property var audioInputs: [{name: "microphone", description: "Microphone"}]
        property var outputNode: audioOutputs[0]
        property var inputNode: audioInputs[0]
        property var selected: null
        property bool scan: false
        property bool networkBusy: false
        property double networkUpdatedAt: 1
        property string networkError: ""
        property string connectionError: ""
        property var connections: []
        property var wirelessNetworks: []
        property var connectionAction: null
        function outputSelected(node) { selected = node; }
        function inputSelected(node) { selected = node; }
        function scanForCommand(enabled) { scan = enabled; }
        function refreshNetwork() { networkBusy = true; }
        function connectionForCommand(id, action) { connectionAction = {id, action}; return {ok: true}; }
    }
    QtObject {
        id: bluetoothDevice
        property string address: "AA:BB:CC:DD:EE:FF"
        property string name: "Test device"
        property bool paired: false
        property bool blocked: false
        property bool connected: false
        property int state: BluetoothDeviceState.Disconnected
        function connect() { connected = true; }
        function disconnect() { connected = false; }
    }
    QtObject {
        id: adapterFixture
        property bool enabled: true
        property bool discovering: false
        property var devices: ({values: [bluetoothDevice]})
    }
    QtObject {
        id: mediaFixture
        property var mediaPlayers: [player]
        property var mediaPlayer: player
        property var services: servicesFixture
        property var deviceControls: devicesFixture
        property var bluetoothAdapter: adapterFixture
        property bool visible: false
        property string page: ""
        property string audioTab: ""
        function openDetail(name, trigger, tab) { page = name; audioTab = tab; }
    }
    QtObject {
        id: notificationFixture
        property int invoked: 0
        property var allEntries: [{historyKey: "session:42", appName: "Test", summary: "A notice", body: "Body",
            notification: {id: 42}, actions: [{text: "Open", action: {identifier: "open", invoke: () => notificationFixture.invoked++}}]}]
        function dismiss(item) { allEntries = allEntries.filter(entry => entry.notification !== item); }
        function dismissAll() { allEntries = []; }
    }
    QtObject {
        id: windowFixture
        property string title: "Test window"
        property bool minimized: false
        property bool activated: false
        function activate() { activated = true; }
        function close() { dockFixture.appGroups = []; }
    }
    QtObject {
        id: dockFixture
        property bool pinned: false
        property int launched: 0
        property var appGroups: [{id: "test-app", desktopEntry: {name: "Test app"}, windows: [windowFixture]}]
        function isPinned(group) { return pinned; }
        function setPinned(group, value) { pinned = value; }
        function launch(group, newWindow) { launched++; }
        function preferredWindow(group) { return group.windows[0]; }
        function moveGroup(id, destination) {}
    }
    ShellCommands { id: commands; indicators: indicatorsFixture; mediaControls: mediaFixture; notificationState: notificationFixture; dockView: dockFixture }
    Timer {
        running: true; interval: 100
        onTriggered: {
            commands.pageCommand("audio", true);
            test.check(mediaFixture.visible && mediaFixture.page === "audio" && mediaFixture.audioTab === "input", "Audio page opens the correct tab");
            test.check(JSON.parse(commands.pageCommand("missing", false)).ok === false, "Unknown page rejected");
            commands.deviceCommand("select", true, "microphone");
            test.check(devicesFixture.selected === devicesFixture.inputNode, "Exact input device selected");
            test.check(JSON.parse(commands.deviceCommand("select", false, "missing")).ok === false, "Missing audio device rejected");
            commands.serviceCommand("power", "set", "power-saver");
            test.check(servicesFixture.lastAction.profile === "power-saver", "Power uses the shared service");
            test.check(JSON.parse(commands.serviceCommand("power", "set", "missing")).ok === false, "Unsupported profile rejected");
            servicesFixture.busy = true;
            test.check(JSON.parse(commands.serviceCommand("night-light", "on", "")).ok === false, "Busy service rejects changes");
            servicesFixture.busy = false;
            commands.serviceCommand("night-light", "on", "");
            test.check(servicesFixture.lastAction.kind === "nightLight" && servicesFixture.lastAction.enabled, "Night Light uses the shared service");
            commands.serviceCommand("awake", "on", "");
            commands.serviceCommand("awake", "on", "");
            test.check(servicesFixture.keepAwake, "Keep Awake on is idempotent");
            commands.serviceCommand("awake", "off", "");
            test.check(!servicesFixture.keepAwake, "Keep Awake off releases the inhibitor");
            test.check(JSON.parse(commands.bluetoothCommand("connect", bluetoothDevice.address)).ok === false && !bluetoothDevice.connected, "Unpaired device never connects");
            bluetoothDevice.paired = true;
            bluetoothDevice.blocked = true;
            test.check(JSON.parse(commands.bluetoothCommand("connect", bluetoothDevice.address)).ok === false, "Blocked device rejected");
            bluetoothDevice.blocked = false;
            commands.bluetoothCommand("connect", bluetoothDevice.address.toLowerCase());
            test.check(bluetoothDevice.connected, "Paired address connects case-insensitively");
            bluetoothDevice.state = BluetoothDeviceState.Connecting;
            test.check(JSON.parse(commands.bluetoothCommand("disconnect", bluetoothDevice.address)).ok === false && bluetoothDevice.connected, "Busy device stays untouched");
            bluetoothDevice.state = BluetoothDeviceState.Connected;
            commands.bluetoothCommand("disconnect", bluetoothDevice.address);
            test.check(!bluetoothDevice.connected, "Selected device disconnects");
            commands.bluetoothCommand("scan", "on");
            test.check(devicesFixture.scan, "Scan delegates to shared ownership logic");
            commands.bluetoothCommand("off", "");
            test.check(!adapterFixture.enabled && JSON.parse(commands.bluetoothCommand("scan", "on")).ok === false, "Disabled adapter cannot scan");
            commands.networkCommand("connect", "uuid");
            test.check(devicesFixture.connectionAction.action === "up" && devicesFixture.connectionAction.id === "uuid", "Connection uses exact UUID");
            commands.networkCommand("refresh", "");
            test.check(JSON.parse(commands.networkCommand("status", "")).busy, "Network reports asynchronous progress");
            test.check(JSON.parse(commands.audioCommand("down", false, 10)).volume === 40, "Volume down uses percent");
            commands.audioCommand("toggle", true, 0);
            test.check(!input.muted && !output.muted, "Input and output mute are independent");
            test.check(JSON.parse(commands.audioCommand("volume", false, 101)).ok === false && output.volume === .4, "Reject unsafe volume");
            commands.mediaCommand("toggle", player.dbusName, 0);
            test.check(player.isPlaying, "Playback reaches selected player");
            commands.mediaCommand("seek", player.dbusName, 30);
            test.check(player.position === 30, "Seek uses seconds");
            test.check(JSON.parse(commands.mediaCommand("next", "", 0)).ok === false, "Unsupported transport is rejected");
            test.check(JSON.parse(commands.mediaCommand("pause", "missing", 0)).ok === false && player.isPlaying, "Missing player never controls another player");
            test.check(JSON.parse(commands.mediaCommand("seek", "", 121)).ok === false && player.position === 30, "Out-of-track seek rejected");
            commands.notificationCommand("invoke", "session:42", "open");
            test.check(notificationFixture.invoked === 1, "Exact notification action invoked");
            test.check(JSON.parse(commands.notificationCommand("invoke", "session:42", "missing")).ok === false, "Missing action rejected");
            commands.notificationCommand("dismiss", "session:42", "");
            test.check(notificationFixture.allEntries.length === 0, "Dismiss only requested notification");
            commands.dockCommand("pin", "test-app", 0);
            commands.dockCommand("launch", "test-app", 0);
            test.check(dockFixture.pinned && dockFixture.launched === 1, "Dock delegates pin and new window actions");
            test.check(JSON.parse(commands.dockCommand("move", "test-app", 2)).ok === false, "Invalid destination rejected");
            const id = JSON.parse(commands.windowCommand("list", "")).windows[0].id;
            commands.windowCommand("minimize", id);
            test.check(windowFixture.minimized, "Minimize selected window");
            commands.windowCommand("activate", id);
            test.check(!windowFixture.minimized && windowFixture.activated, "Activate restores window");
            test.check(JSON.parse(commands.windowCommand("list", "")).windows[0].id === id, "Window ID stable across focus changes");
            commands.windowCommand("close", id);
            test.check(JSON.parse(commands.windowCommand("activate", id)).ok === false, "Stale window IDs rejected");
            console.info(test.failures ? "SHELL_COMMANDS_FAILED" : "SHELL_COMMANDS_PASSED");
            Qt.quit();
        }
    }
}
