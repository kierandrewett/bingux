import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    component Adapter: QtObject {
        property bool enabled: true
        property bool discovering: false
        property var devices: QtObject { property var values: [] }
    }
    Adapter { id: adapter }
    Adapter { id: replacement }
    FloatingWindow {
        id: window
        implicitWidth: 416; implicitHeight: 420
        Rectangle {
            id: canvas
            anchors.fill: parent
            color: Theme.shellSurface
            ControlCentreDetails {
                id: detail
                anchors.fill: parent
                anchors.margins: 16
                indicators: null
                bluetoothAdapter: adapter
                page: "bluetooth"
            }
        }
        TestCase {
            id: test
            property bool saved: false
            when: window.visible
            function test_discovery() {
                report.setText("FAILURES 1\n");
                waitForRendering(detail);
                const settings = findChild(detail, "controlDetailSettings");
                compare(settings.height, 40, "Settings uses the standard full-height control row");
                verify(settings.navigation && settings.hoverEnabled);
                compare(findChild(settings, "controlRowTitle").pixelSize, Theme.fontSize);
                verify(!adapter.discovering, "Inactive page does not scan");
                detail.active = true;
                tryCompare(adapter, "discovering", true);
                const section = findChild(detail, "controlBluetoothNearby");
                const scan = findChild(section, "controlBluetoothDiscover");
                verify(scan !== null, "Scan control belongs to nearby devices, not paired devices");
                mouseClick(scan);
                compare(adapter.discovering, false);
                wait(100);
                verify(!adapter.discovering && section.visible && scan.visible, "Manual stop persists and restart stays accessible");
                mouseClick(scan);
                compare(adapter.discovering, true);
                detail.page = "display";
                compare(adapter.discovering, false);
                detail.page = "bluetooth";
                compare(adapter.discovering, true);
                adapter.enabled = false;
                compare(adapter.discovering, false);
                adapter.enabled = true;
                compare(adapter.discovering, true);
                detail.bluetoothAdapter = replacement;
                compare(adapter.discovering, false);
                compare(replacement.discovering, true);
                detail.active = false;
                compare(replacement.discovering, false);
                replacement.discovering = true; // Another client owns this scan.
                detail.active = true;
                compare(detail.discoveryAdapter, null);
                detail.active = false;
                verify(replacement.discovering, "Closing never stops another client's scan");
                replacement.discovering = false;
                replacement.enabled = false;
                detail.active = true;
                verify(!replacement.discovering && !replacement.enabled, "Opening does not enable Bluetooth");
                replacement.enabled = true;
                wait(150);
                canvas.grabToImage(result => { test.saved = result.saveToFile("/tmp/bingux-bluetooth-discovery.png"); });
                tryCompare(test, "saved", true, 2000);
                detail.active = false;
                report.setText("PASS automatic discovery, separate scan section, manual stop/restart, navigation, power changes, adapter replacement and external scan ownership\nFAILURES 0\n");
            }
        }
    }
}
