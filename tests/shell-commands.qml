import QtQuick
import Quickshell
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
    QtObject { id: mediaFixture; property var mediaPlayers: [player]; property var mediaPlayer: player }
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
