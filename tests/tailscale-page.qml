import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    QtObject {
        id: service
        property bool ready: true
        property bool busy: false
        property string error: ""
        property var state: ({
                power: {
                    profiles: []
                }
            })
        property var lastAction: null
        property var vpns: [
            {
                id: "tailscale",
                name: "Tailscale",
                subtitle: "Private network connected",
                connected: true,
                canToggle: true,
                exitNode: "",
                preferences: {
                    "accept-dns": true,
                    "accept-routes": false,
                    "shields-up": false,
                    "exit-node-allow-lan-access": true
                },
                nodes: Array.from({
                    length: 570
                }, (_, index) => ({
                            id: "node" + index,
                            name: index === 0 ? "Workstation" : "Device " + index,
                            dns: "device" + index + ".example",
                            ips: ["100.64." + Math.floor(index / 250) + "." + (index % 250 + 1)],
                            os: "linux",
                            self: index === 0,
                            online: index < 400,
                            exitOption: index > 0 && index < 10 || index >= 30,
                            provider: index >= 30 ? "Mullvad" : "",
                            city: index >= 30 ? "London" : "",
                            countryCode: index >= 30 ? "GB" : "",
                            country: index >= 30 ? "United Kingdom" : ""
                        }))
            }
        ]
        function action(request) {
            lastAction = request;
        }
    }
    Window {
        id: window
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        width: 416
        height: 640
        Rectangle {
            id: canvas
            anchors.fill: parent
            color: Theme.shellSurface
            ControlCentreExtras {
                id: extras
                anchors.fill: parent
                anchors.margins: 16
                services: service
            }
        }
        TestCase {
            id: test
            property bool saved: false
            when: window.visible
            function checked(actual, expected, message) {
                console.log("CHECK", message || "", actual, expected);
                if (actual !== expected)
                    report.setText("FAIL " + message + ": " + actual + " expected " + expected + "\nFAILURES 1\n");
                compare(actual, expected, message);
            }
            function screenshot(path) {
                saved = false;
                canvas.grabToImage(result => {
                    test.saved = result.saveToFile(path);
                });
                tryCompare(test, "saved", true, 2000);
            }
            function test_management() {
                report.setText("FAILURES 1\n");
                waitForRendering(extras);
                report.setText("navigation\nFAILURES 1\n");
                mouseClick(findChild(extras, "controlVpn_tailscaleNavigation"));
                verify(extras.tailscaleOpen);
                wait(100);
                verify(extras.implicitHeight >= 600, "Tailscale requests the taller page height");
                report.setText("list\nFAILURES 1\n");
                const list = findChild(extras, "tailscaleNodes");
                checked(list.count, 30, "Provider servers are not tailnet devices");
                verify(list.contentItem.children.length < 80, "Node list is virtualised");
                list.positionViewAtIndex(20, ListView.Beginning);
                wait(50);
                const scrollPosition = list.contentY;
                service.vpns = JSON.parse(JSON.stringify(service.vpns));
                wait(50);
                checked(list.contentY, scrollPosition, "Unchanged polls preserve scroll position");
                list.positionViewAtBeginning();
                screenshot("/tmp/bingux-tailscale-devices.png");
                const clipboard = Quickshell.clipboardText;
                try {
                    mouseClick(list.itemAtIndex(0));
                    checked(Quickshell.clipboardText, clipboard, "Left-click does not change the clipboard");
                    mouseClick(list.itemAtIndex(0), 40, 16, Qt.RightButton);
                    const contextMenu = findChild(extras, "tailscaleNodeMenu");
                    tryCompare(contextMenu, "visible", true);
                    checked(contextMenu.menuEntries[0].text, "Copy IP address");
                    contextMenu.activate(contextMenu.menuEntries[0]);
                    checked(contextMenu.visible, false, "Copy closes the context menu");
                    checked(Quickshell.clipboardText, "100.64.0.1", "Context action copies the address");
                    checked(list.itemAtIndex(0).actionLabel, "Copied", "Copy has visible feedback");
                } finally {
                    Quickshell.clipboardText = clipboard;
                }
                mouseClick(findChild(extras, "tailscaleConnectionSwitch"));
                checked(service.lastAction.kind, "vpn");
                checked(service.lastAction.enabled, false);
                service.lastAction = null;
                report.setText("filter\nFAILURES 1\n");
                const search = findChild(extras, "tailscaleSearch");
                mouseClick(search);
                keyClick(Qt.Key_W, Qt.ShiftModifier);
                tryCompare(list, "count", 1);
                report.setText("exit\nFAILURES 1\n");
                const tabsPosition = findChild(extras, "tailscaleTab_exit").mapToItem(canvas, 0, 0);
                mouseClick(findChild(extras, "tailscaleTab_exit"));
                wait(50);
                checked(findChild(extras, "tailscaleTab_exit").mapToItem(canvas, 0, 0).y, tabsPosition.y, "Tabs stay fixed when changing pages");
                tryCompare(list, "count", 549);
                mouseClick(findChild(extras, "tailscaleSource_provider"));
                tryCompare(list, "count", 540);
                verify(list.contentItem.children.length < 80, "Provider list is virtualised");
                mouseClick(search);
                keyClick(Qt.Key_L);
                keyClick(Qt.Key_O);
                keyClick(Qt.Key_N);
                keyClick(Qt.Key_D);
                keyClick(Qt.Key_O);
                keyClick(Qt.Key_N);
                tryCompare(list, "count", 540);
                const table = findChild(extras, "tailscaleTable");
                checked(table.rowHeight, 56, "Location rows have room for server details");
                checked(table.headerVisible, false, "Exit list has no redundant table headers");
                checked(table.tableWidth, table.width, "Exit list fits without horizontal scrolling");
                mouseWheel(list, 80, 60, 0, -120);
                checked(list.contentY - list.originY, 4 * table.rowHeight, "Fixed wheel steps");
                mouseWheel(list, 80, 60, 0, -120, Qt.NoButton, Qt.ShiftModifier);
                checked(table.contentX, 0, "No hidden exit-node columns");
                table.contentX = 0;
                list.positionViewAtBeginning();
                screenshot("/tmp/bingux-tailscale-providers.png");
                search.clearFilter();
                mouseClick(findChild(extras, "tailscaleSource_tailnet"));
                tryCompare(list, "count", 9);
                list.positionViewAtIndex(0, ListView.Beginning);
                wait(100);
                const listHeight = list.height;
                mouseClick(search);
                keyClick(Qt.Key_Down);
                checked(list.currentIndex, 0);
                keyClick(Qt.Key_Return);
                wait(50);
                checked(list.height, listHeight, "Confirmation does not shift the list");
                report.setText("chosen " + list.itemAtIndex(0).title + "\nFAILURES 1\n");
                checked(service.lastAction, null, "Choosing an exit node requires confirmation");
                mouseClick(findChild(extras, "tailscaleApplyExit"));
                report.setText("applied " + JSON.stringify(service.lastAction) + "\nFAILURES 1\n");
                checked(service.lastAction.setting, "exit-node");
                checked(service.lastAction.value, "node1");
                service.vpns = service.vpns.map(row => Object.assign({}, row, {
                        exitNode: "node1"
                    }));
                wait(50);
                checked(findChild(extras, "tailscaleCurrentExit").title, "Device 1");
                findChild(extras, "tailscaleCurrentExit").actionTriggered();
                screenshot("/tmp/bingux-tailscale-exit.png");
                mouseClick(findChild(extras, "tailscaleApplyExit"));
                checked(service.lastAction.value, "", "Stop using clears only the exit node");
                list.positionViewAtIndex(1, ListView.Beginning);
                wait(50);
                mouseClick(list.itemAtIndex(1));
                service.vpns = service.vpns.map(row => Object.assign({}, row, {
                        nodes: row.nodes.map(node => node.id === "node2" ? Object.assign({}, node, {
                                online: false
                            }) : node)
                    }));
                wait(50);
                checked(findChild(extras, "tailscaleApplyExit").enabled, false, "Cannot apply a server that went offline");
                window.width = 360;
                window.height = 492;
                wait(100);
                screenshot("/tmp/bingux-tailscale-compact.png");
                checked(list.height >= 112, true, "Compact table fits two location rows: " + list.height);
                verify(findChild(extras, "tailscaleApplyExit").mapToItem(canvas, 0, 32).y < canvas.height, "Confirmation fits in compact layout");
                screenshot("/tmp/bingux-tailscale-compact.png");
                report.setText("settings\nFAILURES 1\n");
                const settingsTabY = findChild(extras, "tailscaleTab_settings").mapToItem(canvas, 0, 0).y;
                mouseClick(findChild(extras, "tailscaleTab_settings"));
                wait(50);
                checked(findChild(extras, "tailscaleTab_settings").mapToItem(canvas, 0, 0).y, settingsTabY, "Settings keeps tabs in place");
                wait(100);
                mouseClick(findChild(extras, "tailscaleSetting_accept-dnsSwitch"));
                report.setText(JSON.stringify(service.lastAction) + "\nFAILURES 1\n");
                checked(service.lastAction.setting, "accept-dns");
                checked(service.lastAction.enabled, false);
                screenshot("/tmp/bingux-tailscale-settings.png");
                mouseClick(findChild(extras, "controlExtrasBack"));
                verify(!extras.tailscaleOpen);
                report.setText("PASS VPN navigation, separate tailnet and provider lists, 540 virtualised servers, case-insensitive filtering, confirmed exit action and settings layout\nFAILURES 0\n");
            }
        }
    }
}
