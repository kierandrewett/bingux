import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Bluetooth
ShellRoot {
    property string checks: ""
    FileView { id: results; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    QtObject {
        id: bluetoothHeadphones
        property string name: "Headphones"
        property string deviceName: "Headphones"
        property string icon: "audio-headphones"
        property bool connected: true
        property bool paired: true
        property int state: connected ? BluetoothDeviceState.Connected : BluetoothDeviceState.Disconnected
        function connect() { connected = true; }
        function disconnect() { connected = false; }
    }
    QtObject { id: adapter; property bool enabled: true; property bool discovering: false; property var devices: QtObject { property var values: [bluetoothHeadphones] } }
    QtObject { id: audio; property bool muted: false; property real volume: 0.65 }
    QtObject { id: microphoneAudio; property bool muted: false; property real volume: 0.4 }
    QtObject { id: microphoneNode; property bool ready: true; property var audio: microphoneAudio; property string description: "Studio microphone"; property string name: "mic" }
    QtObject { id: sink; property bool ready: true; property var audio: audio }
    QtObject {
        id: indicators
        property string networkState: "wifi"
        property bool audioAvailable: true
        property bool audioMuted: audio.muted
        property real audioVolume: audio.volume
        property var audioSink: sink
        property var audioSource: microphoneNode
        property bool laptopBatteryAvailable: true
        function networkIconName() { return "network-wireless-signal-excellent-symbolic"; }
        function networkAccessibleName() { return "Wireless network connected"; }
        function audioIconName() { return audioMuted ? "audio-volume-muted-symbolic" : "audio-volume-high-symbolic"; }
        function batteryAccessibleName() { return "Battery 84 percent, charging"; }
    }
    QtObject {
        id: player
        property bool positionSupported: true
        property bool lengthSupported: true
        property bool canSeek: true
        property real position: 42
        property real length: 180
        property int uniqueId: 1
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canGoPrevious: true
        property bool canGoNext: true
        property bool isPlaying: false
        property string identity: "Music"
        property string trackTitle: "A Quiet Evening"
        property string trackArtist: "Local library"
        property string trackArtUrl: ""
        function play() { isPlaying = true; }
        function pause() { isPlaying = false; }
        function previous() {}
        function next() {}
    }
    QtObject {
        id: secondPlayer
        property bool positionSupported: true
        property bool lengthSupported: true
        property bool canSeek: true
        property real position: 42
        property real length: 180
        property int uniqueId: 1
        property bool canControl: true
        property bool canPlay: true
        property bool canPause: true
        property bool canGoPrevious: true
        property bool canGoNext: true
        property bool isPlaying: false
        property string identity: "Browser"
        property string trackTitle: "A Quiet Evening"
        property string trackArtist: "Local library"
        property string trackArtUrl: ""
        function play() { isPlaying = true; }
        function pause() { isPlaying = false; }
        function previous() {}
        function next() {}
    }
    Component {
        id: notificationFactory
        QtObject {
            property int id: 1
            property real expireTimeout: 30
            property string appName: "Files"
            property string desktopEntry: ""
            property string appIcon: "system-file-manager"
            property string image: ""
            property string summary: "Download complete"
            property string body: "Your files are ready to open."
            property var actions: []
            property bool tracked: false
            signal closed(int reason)
            function dismiss() {}
            function expire() {}
        }
    }
    NotificationState { id: state }
    NotificationSurface { id: notifications; state: state; notificationCentre: history }
    NotificationHistoryPopup { id: history; notificationSurface: notifications }
    QtObject {
        id: services
        property bool active: false
        property bool ready: true
        property bool busy: false
        property bool keepAwake: false
        property bool doNotDisturb: false
        property string error: ""
        property var actions: []
        property var controls: ({vpn: true, dnd: true, nightLight: false, power: false, awake: false})
        property var vpns: [{id: "mullvad", name: "Mullvad", connected: false, subtitle: "Disconnected", canToggle: true}, {id: "tailscale", name: "Tailscale", connected: true, subtitle: "Exit node active", canToggle: true}]
        property var state: ({dndAvailable: true, nightLightAvailable: true, nightLight: false, nightLightActive: false, awakeAvailable: true, power: {available: true, profile: "balanced", profiles: ["power-saver", "balanced", "performance"]}})
        function showControl(name) { return !!controls[name]; }
        function setControl(name, value) { controls = Object.assign({}, controls, {[name]: value}); }
        function action(value) { actions = actions.concat([value]); }
        function toggleAwake() { keepAwake = !keepAwake; }
    }
    ControlCentre { id: centre; services: services; indicators: indicators; bluetoothAdapter: adapter }
    Timer { id: quitAfterUnmap; interval: 400; onTriggered: Qt.quit() }
    TestCase {
        parent: centre.contentItem
        name: "ControlCentre"
        when: true
        function check(value, message) { checks += "CHECK " + value + " " + message + "\n"; results.setText(checks); verify(value, message); }
        function equal(actual, expected, message) { checks += "EQUAL " + actual + " " + expected + " " + message + "\n"; results.setText(checks); compare(actual, expected, message); }
        function test_zz_notificationVariants() {
            const invoked = {reply: 0, defaultAction: 0};
            const avatar = notificationFactory.createObject(state, {id: 501, image: Quickshell.shellPath("avatar.svg"), summary: "Message from Marc", body: "Cooking"});
            const screenshot = notificationFactory.createObject(state, {id: 502, image: Quickshell.shellPath("avatar.svg"), summary: "Screenshot saved", body: "A screenshot with a preview"});
            const actionable = notificationFactory.createObject(state, {id: 503, summary: "Choose an action", body: "A longer notification body. ".repeat(14), actions: [
                {identifier: "default", text: "Open", invoke: () => invoked.defaultAction++},
                {identifier: "reply", text: "Reply to this message using a deliberately long action label that needs more than one line", invoke: () => invoked.reply++},
                {identifier: "later", text: "Remind me later", invoke: () => {}}
            ]});
            state.accept(avatar); state.accept(screenshot); state.accept(actionable);
            history.visible = true;
            wait(700);
            const head = findChild(notifications.contentItem, "notificationCard");
            const cards = head.parent.children.filter(item => item.objectName === "notificationCard");
            const actionCard = cards.find(item => item.notificationId === 503);
            const reply = findChild(actionCard, "notificationAction_reply");
            check(reply.width <= actionCard.width && reply.height > 36, "long action labels wrap inside the card");
            const actionPoint = reply.mapToItem(notifications.viewport, 0, 0);
            notifications.viewport.contentY = Math.max(0, Math.min(notifications.viewport.contentHeight - notifications.viewport.height,
                notifications.viewport.contentY + actionPoint.y + reply.height - notifications.viewport.height + 8));
            wait(80);
            mouseClick(reply, reply.width / 2, reply.height / 2);
            equal(invoked.reply, 1, "notification button invokes its own action exactly once");
            equal(invoked.defaultAction, 0, "action click does not invoke the card default action");
            check(!actionCard.groupExpanded, "action click does not accidentally expand the stack");
            notifications.toggleGroup(actionCard.groupKey);
            wait(350);
            const ordered = cards.slice().sort((a, b) => a.groupDepth - b.groupDepth);
            for (let i = 0; i < ordered.length; ++i) {
                equal(ordered[i].height, ordered[i].naturalHeight, "expanded mixed card uses its own content height");
                if (i > 0) check(ordered[i].y >= ordered[i - 1].y + ordered[i - 1].height, "mixed cards do not overlap when expanded");
            }
            const small = findChild(cards.find(item => item.notificationId === 501), "notificationImagePreview");
            const large = findChild(cards.find(item => item.notificationId === 502), "notificationImagePreview");
            equal(small.width, 40, "profile image remains avatar-sized in mixed stack");
            tryVerify(() => large.width > 40 && large.height > 40, 5000, "screenshot retains its large preview after its image loads");
            actionable.actions = actionable.actions.concat([{identifier: "save", text: "Save", invoke: () => {}}]);
            wait(250);
            check(findChild(actionCard, "notificationAction_save") !== null, "live action updates appear without replacing the card");
            equal(actionCard.slideOffset, 0, "content updates do not replay the entrance");
            state.dismissAll();
            wait(650);
            history.visible = false;
            wait(250);
        }
        function test_integration() {
            if (Quickshell.env("BINGUX_NOTIFICATION_CONTENT_ONLY")) return;
            const first = notificationFactory.createObject(state, {id: 1, image: Quickshell.shellPath("avatar.svg")});
            state.accept(first);
            wait(650);
            const viewport = notifications.viewport;
            const homeY = viewport.y;
            centre.mediaPlayers = [player];
            centre.visible = true;
            wait(350);
            check(!history.visible, "Control Centre does not open notification history");
            equal(viewport.y, homeY, "opening controls does not move desktop notifications");
            const bluetooth = findChild(centre.contentItem, "controlBluetooth");
            mouseClick(bluetooth, bluetooth.width / 2, bluetooth.height - 18);
            check(!centre.detailOpen, "connectivity row remains passive");
            const power = findChild(bluetooth, "controlBluetoothSwitch");
            mouseClick(power, power.width / 2, power.height / 2);
            check(!adapter.enabled && !centre.detailOpen, "power switch does not navigate");
            mouseClick(power, power.width / 2, power.height / 2);
            const navigation = findChild(bluetooth, "controlBluetoothNavigation");
            mouseClick(navigation, 16, 16);
            wait(400);
            check(centre.detailOpen, "chevron opens the detail panel");
            check(centre.popupHeight <= centre.height * 0.8 + 1, "height remains below eighty percent");
            check(centre.panelY + centre.popupHeight <= centre.dockSafeBottom + 1, "panel stays above dock boundary");
            const back = findChild(centre.contentItem, "controlDetailBack");
            check(back !== null && back.visible, "detail back button is available before clicking");
            mouseClick(back, 16, 16);
            wait(350);
            const microphone = findChild(centre.contentItem, "controlMicrophoneVolume");
            check(microphone !== null && microphone.visible, "overview microphone slider is available after returning");
            mouseClick(microphone, microphone.width * 0.65, microphone.height / 2);
            check(microphoneAudio.volume > 0.5, "overview microphone slider controls input gain");
            const output = findChild(centre.contentItem, "controlVolume");
            const boost = findChild(output, "sliderBoostFill");
            audio.volume = 1; wait(20);
            check(boost.width === 0, "100 percent volume has no red boost fill");
            audio.volume = 1.2; wait(20);
            check(boost.width > 0 && Math.abs(boost.width / output.background.width - 0.2 / 1.5) < 0.01,
                "only the filled portion above 100 percent is red");
            audio.volume = 0.65; wait(20);
            check(boost.width === 0, "red boost fill clears below 100 percent");
            check(findChild(microphone, "sliderBoostFill").width === 0, "microphone uses normal seek fill");
            centre.visible = false;
            wait(250);
            state.archiveToasts();
            history.visible = true;
            wait(350);
            check(!centre.visible && history.visible, "history opens independently of controls");
            check(!viewport.transferring, "archived history does not restart toast animation");
            const card = findChild(notifications.contentItem, "notificationCard");
            const preview = findChild(card, "notificationImagePreview");
            equal(preview.width, 40, "avatar uses compact sizing");
            equal(preview.parent, card, "avatar sits beside text");
            equal(card.slideOffset, 0, "archived card is settled immediately");
            const clear = findChild(notifications.contentItem, "controlClearNotifications");
            mouseClick(clear, clear.width / 2, clear.height / 2);
            wait(700);
            equal(state.allEntries.length, 0, "clear all removes history");
            check(!history.visible, "empty history closes its popup");
            wait(250);
        }
        function cleanupTestCase() {
            results.setText(checks + "FAILURES " + qtest_results.failCount);
            centre.visible = false;
            history.visible = false;
            if (!Quickshell.env("BINGUX_CONTROL_TEST_NO_QUIT")) quitAfterUnmap.start();
        }
    }
}
