import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Bluetooth

ShellRoot {
    property string checks: ""
    FileView {
        id: results
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    QtObject {
        id: bluetoothHeadphones
        property string name: "Headphones"
        property string deviceName: "Headphones"
        property string icon: "audio-headphones"
        property bool connected: true
        property bool paired: true
        property int state: connected ? BluetoothDeviceState.Connected : BluetoothDeviceState.Disconnected
        function connect() {
            connected = true;
        }
        function disconnect() {
            connected = false;
        }
    }
    QtObject {
        id: adapter
        property bool enabled: true
        property bool discovering: false
        property var devices: QtObject {
            property var values: [bluetoothHeadphones]
        }
    }
    QtObject {
        id: audio
        property bool muted: false
        property real volume: 0.65
    }
    QtObject {
        id: microphoneAudio
        property bool muted: false
        property real volume: 0.4
    }
    QtObject {
        id: microphoneNode
        property bool ready: true
        property var audio: microphoneAudio
        property string description: "Studio microphone"
        property string name: "mic"
    }
    QtObject {
        id: sink
        property bool ready: true
        property var audio: audio
    }
    QtObject {
        id: indicators
        property string networkState: "wifi"
        property bool audioAvailable: true
        property bool audioMuted: audio.muted
        property real audioVolume: audio.volume
        property var audioSink: sink
        property var audioSource: microphoneNode
        property bool laptopBatteryAvailable: true
        function networkIconName() {
            return "network-wireless-signal-excellent-symbolic";
        }
        function networkAccessibleName() {
            return "Wireless network connected";
        }
        function audioIconName() {
            return audioMuted ? "audio-volume-muted-symbolic" : "audio-volume-high-symbolic";
        }
        function batteryAccessibleName() {
            return "Battery 84 percent, charging";
        }
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
        function play() {
            isPlaying = true;
        }
        function pause() {
            isPlaying = false;
        }
        function previous() {
        }
        function next() {
        }
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
        function play() {
            isPlaying = true;
        }
        function pause() {
            isPlaying = false;
        }
        function previous() {
        }
        function next() {
        }
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
            function dismiss() {
            }
            function expire() {
            }
        }
    }
    NotificationState {
        id: state
        historyDirectory: Quickshell.shellPath("notification-fixture")
    }
    QtObject {
        id: services
        property bool active: false
        property bool statusVisible: false
        property bool ready: true
        property bool busy: false
        property bool keepAwake: false
        property bool doNotDisturb: false
        property string error: ""
        property var actions: []
        property var controls: ({
                vpn: true,
                dnd: true,
                nightLight: false,
                power: false,
                awake: false
            })
        property var vpns: [
            {
                id: "mullvad",
                name: "Mullvad",
                connected: false,
                subtitle: "Disconnected",
                canToggle: true
            },
            {
                id: "tailscale",
                name: "Tailscale",
                connected: true,
                subtitle: "Exit node active",
                canToggle: true
            }
        ]
        property var state: ({
                dndAvailable: true,
                nightLightAvailable: true,
                nightLight: false,
                nightLightActive: false,
                awakeAvailable: true,
                power: {
                    available: true,
                    profile: "balanced",
                    profiles: ["power-saver", "balanced", "performance"]
                }
            })
        function showControl(name) {
            return !!controls[name];
        }
        function setControl(name, value) {
            controls = Object.assign({}, controls, {
                [name]: value
            });
        }
        function action(value) {
            actions = actions.concat([value]);
        }
        function toggleAwake() {
            keepAwake = !keepAwake;
        }
    }
    PanelWindow {
        id: testHost
        visible: true
        implicitWidth: 1280
        implicitHeight: 1080
        color: "transparent"
    }
    SystemIndicators {
        id: barIndicators
        parent: testHost.contentItem
        services: services
        x: 8
        y: 8
    }
    ControlCentre {
        id: centre
        hostItem: testHost.contentItem
        services: services
        indicators: indicators
        bluetoothAdapter: adapter
    }
    ControlCentre {
        id: nativeCentre
        services: services
        indicators: indicators
        bluetoothAdapter: adapter
        mediaPlayers: []
    }
    TestCase {
        parent: centre.contentItem
        name: "ControlCentreServices"
        when: true
        function check(value, message) {
            checks += "CHECK " + value + " " + message + "\n";
            results.setText(checks);
            verify(value, message);
        }
        function visibleTitle(item, text) {
            if (item.objectName === "controlRowTitle" && item.visible && item.text === text)
                return item;
            for (const child of item.children || []) {
                const found = visibleTitle(child, text);
                if (found)
                    return found;
            }
            return null;
        }
        function test_scrolling_names_and_stable_width() {
            if (Quickshell.env("BINGUX_CONTROL_STATUS_ONLY"))
                return;
            centre.visible = true;
            const detail = findChild(centre.contentItem, "controlDetailPage");
            detail.active = false; // Fixture data only; no discovery or PipeWire tracking.
            const longName = "Studio device with a very long descriptive name that needs scrolling to read completely";
            bluetoothHeadphones.name = longName;
            microphoneNode.description = longName;
            detail.audioOutputs = [microphoneNode];
            detail.audioInputs = [microphoneNode];
            detail.connections = [
                {
                    uuid: "fixture",
                    type: "802-11-wireless",
                    name: longName,
                    connected: true
                }
            ];
            detail.wirelessNetworks = [];
            for (const page of ["bluetooth", "audio", "input", "network"]) {
                centre.openDetail(page === "input" ? "audio" : page, null, page === "input" ? "input" : "output");
                wait(300);
                const title = visibleTitle(detail, longName);
                check(title !== null && title.overflow > 0, page + " long name is elided at rest");
                mouseMove(title, title.width / 2, title.height / 2);
                wait(800);
                check(Theme.reducedMotion ? title.offset === 0 : title.offset > 0, page + " hover marquee respects motion preference");
                mouseMove(findChild(detail, "controlDetailBack"), 10, 10);
                wait(50);
                check(title.offset === 0 && !title.revealed, page + " marquee resets on pointer exit");
            }
            const rows = findChild(detail, "controlDeviceRows");
            const overviewRows = findChild(centre.contentItem, "controlOverviewRows");
            const expectedWidth = rows.width;
            detail.audioOutputs = Array(30).fill(microphoneNode);
            centre.openDetail("audio");
            for (let frame = 0; frame < 16; frame++) {
                wait(16);
                verify(Math.abs(rows.width - expectedWidth) < 1 && Math.abs(overviewRows.width - expectedWidth) < 1, "row width during growing page transition");
            }
            const list = findChild(detail, "controlDeviceList");
            const bar = findChild(detail, "controlDeviceScrollbar");
            check(list.scrollable && bar.x >= list.x + list.width, "overflow scrollbar occupies outer padding, outside device rows");
            if (Quickshell.env("BINGUX_CONTROL_SCREENSHOT"))
                centre.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_CONTROL_SCREENSHOT") + ".scrolling.png"));
            wait(100);
            centre.openDetail("bluetooth");
            for (let frame = 0; frame < 16; frame++) {
                wait(16);
                verify(Math.abs(rows.width - expectedWidth) < 1, "row width during shrinking page transition");
            }
            check(!list.scrollable, "short detail page no longer needs scrolling");
            check(Math.abs(rows.width - expectedWidth) < 1, "row width stays constant across scrollbar changes");
            centre.visible = false;
            wait(300);
        }
        function test_status_indicators() {
            const actionCount = services.actions.length;
            for (const name of ["vpn", "dnd", "nightLight", "power", "awake"])
                services.setControl(name, true);
            wait(50);
            check(barIndicators.extraIndicators.filter(row => row.key !== "microphone").length === 5, "all opted-in controls have top-bar indicators");
            for (const name of ["vpn", "dnd", "nightLight", "power", "awake"])
                check(findChild(barIndicators, "controlStatus_" + name) !== null, "status icon renders: " + name);
            check(services.statusVisible && !centre.visible, "status monitoring continues with Control Centre closed");
            check(barIndicators.extraIndicators.find(row => row.key === "vpn").active, "connected VPN has an active indicator");
            if (Quickshell.env("BINGUX_CONTROL_SCREENSHOT"))
                barIndicators.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_CONTROL_SCREENSHOT") + ".indicators.png"));
            wait(100);
            services.vpns = services.vpns.map(vpn => Object.assign({}, vpn, {
                    connected: false,
                    subtitle: "Disconnected"
                }));
            check(!barIndicators.extraIndicators.find(row => row.key === "vpn").active && !barIndicators.extraStatusDescription.includes("VPN: Disconnected"), "inactive VPN is omitted from the top-bar description");
            const vpnIcon = findChild(barIndicators, "controlStatus_vpn");
            check(!vpnIcon.shown, "disconnected VPN exits");
            wait(Theme.statusIndicatorMotion + 80);
            check(!vpnIcon.visible && vpnIcon.width === 0, "removed VPN leaves no empty slot");
            const dndIcon = findChild(barIndicators, "controlStatus_dnd");
            services.doNotDisturb = false;
            wait(Theme.statusIndicatorMotion + 80);
            check(!dndIcon.visible && dndIcon.width === 0, "disabled DND is hidden");
            services.doNotDisturb = true;
            check(barIndicators.extraIndicators.find(row => row.key === "dnd").active, "Do Not Disturb indicator reflects live state");
            wait(60);
            check(Theme.reducedMotion ? dndIcon.reveal === 1 : dndIcon.reveal > 0 && dndIcon.reveal < 1, "DND animates into its slot");
            services.doNotDisturb = false;
            wait(40);
            services.doNotDisturb = true;
            wait(Theme.statusIndicatorMotion + 80);
            check(dndIcon.visible && dndIcon.reveal === 1, "removal can reverse without losing the icon");
            check(findChild(dndIcon, "statusIcon").color.toString() === Theme.text.toString(), "active indicators use uniform foreground, not accent");
            services.setControl("nightLight", false);
            wait(50);
            check(!barIndicators.extraIndicators.some(row => row.key === "nightLight"), "removing a control removes its indicator");
            wait(Theme.statusIndicatorMotion + 80);
            check(!findChild(barIndicators, "controlStatus_nightLight").visible, "removed control finishes its exit");
            check(services.actions.length === actionCount, "status rendering never changes system settings");
        }
        function test_z_opening_size() {
            if (Quickshell.env("BINGUX_CONTROL_STATUS_ONLY"))
                return;
            centre.visible = true;
            wait(300);
            const fullHeight = centre.popupHeight;
            const fullWidth = centre.popupWidth;
            centre.visible = false;
            wait(300);
            services.setControl("nightLight", true);
            wait(20);
            centre.visible = true;
            wait(20);
            const openingHeight = centre.popupHeight;
            wait(300);
            check(centre.popupHeight > fullHeight, "customised controls determine the opening height");
            check(Math.abs(centre.popupHeight - openingHeight) < 1 && centre.popupWidth === fullWidth, "opening uses final dimensions without a second resize");
            centre.visible = false;
            wait(300);
            nativeCentre.visible = true;
            wait(20);
            const nativeOpeningHeight = nativeCentre.popupHeight;
            const nativeOpeningWidth = nativeCentre.body.width;
            wait(300);
            check(Math.abs(nativeCentre.popupHeight - nativeOpeningHeight) < 1 && nativeCentre.body.width === nativeOpeningWidth, "native window opens at final size before reveal completes");
            nativeCentre.visible = false;
            wait(300);
        }
        function test_optional_controls() {
            if (Quickshell.env("BINGUX_CONTROL_STATUS_ONLY"))
                return;
            centre.mediaPlayers = [];
            centre.visible = true;
            wait(500);
            const vpn = findChild(centre.contentItem, "controlVpn");
            check(vpn.visible && vpn.subtitle === "Tailscale", "overview shows connected VPN service");
            check(!findChild(centre.contentItem, "controlNightLight").visible, "Night Light is opt-in");
            check(!findChild(centre.contentItem, "controlPower").visible, "Power mode is opt-in");
            const nav = findChild(centre.contentItem, "controlVpnNavigation");
            mouseClick(nav);
            wait(250);
            const extra = findChild(centre.contentItem, "controlExtrasPage");
            check(extra.visible && centre.detailPage === "vpn", "VPN arrow opens its detail page");
            const tail = findChild(extra, "controlVpn_tailscale");
            check(tail.subtitle === "Exit node active", "exit node status is explicit");
            mouseClick(findChild(tail, "controlVpn_tailscaleSwitch"));
            check(services.actions.length === 1 && services.actions[0].id === "tailscale" && !services.actions[0].enabled, "VPN switch requests only its own service action");
            if (Quickshell.env("BINGUX_CONTROL_SCREENSHOT"))
                centre.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_CONTROL_SCREENSHOT") + ".vpn.png"));
            mouseClick(findChild(extra, "controlExtrasBack"));
            wait(250);
            const customise = findChild(centre.contentItem, "controlCustomise");
            mouseClick(customise);
            wait(250);
            check(centre.detailPage === "customise", "customisation stays inside the control centre");
            for (const name of ["nightLight", "power", "awake"]) {
                mouseClick(findChild(extra, "controlOption_" + name + "Switch"));
                check(services.showControl(name), "optional control can be added: " + name);
            }
            check(services.actions.length === 1 && !services.keepAwake, "adding controls does not enable their features");
            if (Quickshell.env("BINGUX_CONTROL_SCREENSHOT"))
                centre.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_CONTROL_SCREENSHOT") + ".customise.png"));
            mouseClick(findChild(extra, "controlExtrasBack"));
            wait(300);
            check(findChild(centre.contentItem, "controlNightLight").visible, "opted-in control appears on overview");
            mouseClick(findChild(centre.contentItem, "controlDndSwitch"));
            check(services.actions[1].kind === "dnd" && services.actions[1].enabled, "Do Not Disturb uses its own explicit action");
            mouseClick(findChild(centre.contentItem, "controlKeepAwakeSwitch"));
            check(services.keepAwake, "Keep Awake can be enabled explicitly");
            mouseClick(findChild(centre.contentItem, "controlKeepAwakeSwitch"));
            check(!services.keepAwake, "Keep Awake can be stopped");
            if (Quickshell.env("BINGUX_CONTROL_SCREENSHOT"))
                centre.body.parent.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_CONTROL_SCREENSHOT") + ".overview.png"));
            mouseClick(findChild(centre.contentItem, "controlPowerNavigation"));
            wait(250);
            check(centre.visible && centre.detailPage === "power", "power arrow opens the power mode page");
            const saver = findChild(extra, "controlPower_power-saver");
            check(saver !== null && saver.visible, "power mode choices are visible");
            mouseClick(saver);
            check(services.actions[2].kind === "power" && services.actions[2].profile === "power-saver", "power mode selection is explicit");
            state.doNotDisturb = true;
            const notification = notificationFactory.createObject(state, {
                id: 998
            });
            state.accept(notification);
            check(state.allEntries.length === 1 && state.visibleEntries.length === 0, "Do Not Disturb saves history without a banner");
            state.doNotDisturb = false;
            check(state.visibleEntries.length === 0, "leaving Do Not Disturb does not replay old banners");
            centre.closeDetail();
            wait(250);
            services.setControl("nightLight", false);
            check(!findChild(centre.contentItem, "controlNightLight").visible, "optional control can be removed");
            centre.visible = false;
            wait(300);
        }
        function cleanupTestCase() {
            results.setText(checks + "FAILURES " + qtest_results.failCount);
            Qt.quit();
        }
    }
}
