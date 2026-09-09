import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 416; implicitHeight: 560
        Rectangle {
            id: canvas
            anchors.fill: parent
            color: Theme.shellSurface
            ControlCentreDetails {
                id: detail
                anchors.fill: parent
                anchors.margins: 16
                indicators: null
                active: false // Synthetic data only; never activate a real connection.
                page: "network"
                connections: [
                    {uuid: "home", type: "802-3-ethernet", name: "Wired connection", connected: true},
                    {uuid: "vpn", type: "tun", name: "tailscale0", connected: true},
                    {uuid: "wifi", type: "802-11-wireless", name: "Home Wi-Fi", connected: false},
                    {uuid: "work", type: "802-11-wireless", name: "Office", connected: false}
                ]
                wirelessNetworks: [{name: "Guest network", security: "WPA2", signal: 80, connected: false},
                                   {name: "Coffee shop", security: "Open", signal: 45, connected: false}]
            }
        }
        TestCase {
            id: test
            property bool saved: false
            when: window.visible
            function test_network() {
                report.setText("FAILURES 1\n");
                waitForRendering(detail);
                compare(detail.networkSections.current.length, 1);
                compare(detail.networkSections.other.length, 2);
                compare(findChild(detail, "controlOtherNetworks"), null, "Saved networks are a regular section, not a disclosure button");
                wait(100);
                canvas.grabToImage(result => { test.saved = result.saveToFile("/tmp/bingux-network-layout.png"); });
                tryCompare(test, "saved", true, 2000);
                detail.wifiRadioState = "disabled";
                wait(50);
                verify(findChild(detail, "controlWifiDisabled").visible, "disabled Wi-Fi shows an enable prompt");
                verify(!findChild(detail, "controlWifiPower").checked, "disabled Wi-Fi switch is off");
                const blocked = detail.connectionForCommand("wifi", "up");
                verify(!blocked.ok && blocked.error.indexOf("Wi-Fi is off") >= 0, "disabled Wi-Fi blocks a misleading connection attempt");
                detail.wifiRadioState = "enabled";
                wait(50);
                verify(!findChild(detail, "controlWifiDisabled").visible, "enabling Wi-Fi removes the prompt");
                report.setText("PASS physical network grouping, Wi-Fi radio prompt, standard saved-network section and settings row\nFAILURES 0\n");
            }
        }
    }
}
