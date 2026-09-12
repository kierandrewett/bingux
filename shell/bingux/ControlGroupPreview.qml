import QtQuick
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

ControlCentreOverview {
    id: root
    required property string widgetId
    property var sourceDesktop: DesktopEditing.desktop
    desktop: {
        const result = Object.assign({}, sourceDesktop);
        const groups = JSON.parse(JSON.stringify(sourceDesktop.controlLayout || ControlLayout.defaults()));
        groups.groups["control-centre"] = [widgetId];
        result.controlLayout = groups;
        result.layout = {
            "top-left": [],
            "top-center": [],
            "top-right": [],
            dock: [],
            sidebar: []
        };
        const container = DesktopLayout.zone(sourceDesktop.layout || {}, widgetId);
        if (container)
            result.layout[container] = [widgetId];
        return result;
    }
    editingEnabled: false
    accountImageSource: ""
    widgetLayout: QtObject {
        function hostFor(item) {
            return root;
        }
        function windowFor(item) {
            return root.barWindow;
        }
        function controlRow(item) {
            return 0;
        }
        function controlColumn(item) {
            return 0;
        }
    }
    indicators: QtObject {
        property var audioSink: sampleAudio
        property var audioSource: sampleMicrophone
        property bool laptopBatteryAvailable: true
        property string networkState: "wifi"
        function batteryAccessibleName() {
            return "Battery 84 percent, charging";
        }
        function audioIconName() {
            return "audio-volume-high-symbolic";
        }
        function networkIconName() {
            return "network-wireless-signal-excellent-symbolic";
        }
        function networkAccessibleName() {
            return "Wireless network connected";
        }
    }
    services: QtObject {
        property var vpns: [
            {
                name: "Private network",
                connected: true
            }
        ]
        property bool busy: false
        property bool doNotDisturb: false
        property bool keepAwake: false
        property string error: ""
        property var state: ({
                dndAvailable: true,
                nightLightAvailable: true,
                nightLight: false,
                nightLightActive: false,
                awakeAvailable: true,
                power: {
                    available: true,
                    profile: "balanced"
                }
            })
        function showControl(name) {
            return (root.sourceDesktop.controlCentre || {
                    vpn: true,
                    dnd: true,
                    nightLight: false,
                    power: false,
                    awake: false
                })[name] === true;
        }
        function action(command) {
        }
        function toggleAwake() {
        }
    }
    bluetoothAdapter: QtObject {
        property bool enabled: true
    }
    QtObject {
        id: sampleAudio
        property bool ready: true
        property var audio: QtObject {
            property real volume: 0.6
            property bool muted: false
        }
    }
    QtObject {
        id: sampleMicrophone
        property bool ready: true
        property var audio: QtObject {
            property real volume: 0.4
            property bool muted: false
        }
    }
}
