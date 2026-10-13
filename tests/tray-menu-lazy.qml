import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
        blockWrites: true
    }
    // Explicitly schedule the normal QtTest runner after the window is shown.
    // Avoid re-entry if the automatic QtTest scheduler has already started it.
    Timer {
        interval: 100
        running: window.visible && !testCase.completed && !testCase.running
        repeat: true
        onTriggered: {
            testCase.qtest_run();
        }
    }
    Timer {
        id: rowsDelivery
        property var rows: []
        interval: 100
        onTriggered: testCase.delayedRows = rows
    }
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 420
        implicitHeight: 100
        Tray {
            id: tray
            parentWindow: window
            serviceEnabled: false
        }
        TestCase {
            id: testCase
            name: "TrayMenuLazy"
            parent: window.contentItem
            when: window.visible
            property var delayedRows: []
            property var provider: null
            property var passedCases: []
            property string stage: ""
            onDelayedRowsChanged: if (provider)
                provider.testActions = delayedRows
            Component {
                id: providerFactory
                QtObject {
                    property string id: "cold"
                    property string title: id
                    property string tooltipTitle: id
                    property string icon: ""
                    property var menu: null
                    property var testHandle: null
                    property var testActions: []
                    property bool hasMenu: true
                    property bool onlyMenu: true
                    function activate() {
                    }
                    function secondaryActivate() {
                    }
                    function scroll() {
                    }
                }
            }
            property var handle: ({
                    token: 1
                })
            function item(id) {
                return findChild(tray, "trayItem-" + id);
            }
            function menu(id) {
                return findChild(item(id), "trayItemMenu");
            }
            function queueRows(rows) {
                rowsDelivery.rows = rows;
                rowsDelivery.restart();
            }
            function sample(id, menuHandle, onlyMenu) {
                if (!provider)
                    provider = providerFactory.createObject(tray, {
                        id,
                        testHandle: menuHandle,
                        testActions: delayedRows,
                        onlyMenu
                    });
                provider.testHandle = menuHandle;
                provider.testActions = delayedRows;
                return provider;
            }
            function initTestCase() {
                console.info("TRAY_TEST_STARTED", report.path);
                report.setText("RUNNING");
            }
            function cleanupTestCase() {
                console.info("TRAY_TEST_FINISHED", qtest_results.failCount);
                report.setText(JSON.stringify({
                    finished: true,
                    failures: qtest_results.failCount,
                    skipped: qtest_results.skipCount,
                    passedCases
                }));
                wait(150);
                Qt.quit();
            }
            function init() {
                console.info("TRAY_CASE", qtest_results.functionName);
                stage = "initialisation";
                mouseMove(window.contentItem, 400, 80);
                tray.trayItems = [];
                wait(50);
                handle = ({
                        token: Date.now()
                    });
                delayedRows = [];
                tray.trayItems = [sample("cold", handle, true)];
                tryVerify(() => item("cold") !== null);
                wait(30);
            }
            function cleanup() {
                if (qtest_results.failed)
                    console.error("TRAY_CASE_FAILED", qtest_results.functionName, stage);
                else {
                    passedCases = passedCases.concat([qtest_results.functionName]);
                    console.info("TRAY_CASE_PASS", qtest_results.functionName);
                }
                rowsDelivery.stop();
                tray.trayItems = [];
                wait(50);
                if (provider)
                    provider.destroy();
                provider = null;
            }
            function test_cold_click_opens_when_delayed_rows_arrive() {
                stage = "cold absence";
                verify(menu("cold") === null, "cold menu is not constructed before intent");
                stage = "click loads menu";
                mouseClick(item("cold"));
                tryVerify(() => menu("cold") !== null);
                verify(menu("cold").anchorsBoundAtCompletion, "menu anchors are bound before completion");
                verify(!menu("cold").visible, "loading rows alone does not show an empty menu");
                queueRows([
                    {
                        text: "Delayed action",
                        isSeparator: false,
                        enabled: true
                    }
                ]);
                stage = "provider rows open menu";
                tryCompare(menu("cold"), "visible", true, 3000);
                tryCompare(menu("cold"), "hasMenuItems", true, 3000);
            }
            function test_hover_prewarm_and_warm_reopen() {
                stage = "hover loads menu";
                mouseMove(item("cold"));
                tryVerify(() => menu("cold") !== null);
                delayedRows = [
                    {
                        text: "Warm action",
                        isSeparator: false,
                        enabled: true
                    }
                ];
                tray.trayItems = [sample("cold", handle, true)];
                stage = "hover rows prewarm";
                tryCompare(menu("cold"), "hasMenuItems", true);
                verify(!menu("cold").visible, "hover prewarms without opening");
                item("cold").openTrayMenu();
                tryCompare(menu("cold"), "visible", true);
                menu("cold").visible = false;
                item("cold").openTrayMenu();
                tryCompare(menu("cold"), "visible", true);
            }
            function test_prewarmed_empty_click_waits_for_rows() {
                stage = "hover empty loads menu";
                mouseMove(item("cold"));
                tryVerify(() => menu("cold") !== null);
                mouseClick(item("cold"));
                verify(!menu("cold").visible, "prewarmed empty menu does not open early");
                queueRows([
                    {
                        text: "Arrived later",
                        isSeparator: false,
                        enabled: true
                    }
                ]);
                stage = "loaded empty queued click opens";
                tryCompare(menu("cold"), "visible", true, 3000);
            }
            function test_escape_and_outside_pointer_cancel_pending_open() {
                stage = "Escape cancels";
                item("cold").forceActiveFocus();
                item("cold").openTrayMenu();
                keyClick(Qt.Key_Escape);
                delayedRows = [
                    {
                        text: "Canceled",
                        isSeparator: false,
                        enabled: true
                    }
                ];
                tray.trayItems = [sample("cold", handle, true)];
                wait(80);
                verify(!menu("cold")?.visible, "Escape cancels pending keyboard open");
                stage = "outside cancels";
                delayedRows = [];
                mouseMove(item("cold"), item("cold").width / 2, item("cold").height / 2);
                mouseClick(item("cold"));
                verify(item("cold").trayMenuOpenRequested, "pointer click queues an empty menu");
                mouseMove(window.contentItem, 400, 80);
                mouseClick(window.contentItem, 400, 80);
                compare(item("cold").trayMenuOpenRequested, false, "outside movement cancels before rows arrive");
                delayedRows = [
                    {
                        text: "Canceled outside",
                        isSeparator: false,
                        enabled: true
                    }
                ];
                tray.trayItems = [sample("cold", handle, true)];
                wait(80);
                verify(!menu("cold")?.visible, "outside pointer movement cancels pending open");
            }
            function test_empty_and_keyboard_cold_open() {
                stage = "empty closed";
                item("cold").openTrayMenu();
                tryVerify(() => menu("cold") !== null);
                wait(80);
                verify(!menu("cold").visible, "empty menu stays closed");
                item("cold").cancelTrayMenuOpen();
                for (const key of [Qt.Key_Return, Qt.Key_Space]) {
                    delayedRows = [];
                    menu("cold").visible = false;
                    item("cold").forceActiveFocus();
                    verify(!menu("cold").visible, "focus alone does not open a menu");
                    keyClick(key);
                    compare(item("cold").trayMenuOpenRequested, true, "keyboard queues the empty menu");
                    queueRows([
                        {
                            text: "Keyboard action",
                            isSeparator: false,
                            enabled: true
                        }
                    ]);
                    stage = "keyboard opens after rows";
                    tryCompare(menu("cold"), "visible", true, 3000);
                }
            }
            function test_replacement_and_removal_cancel_pending_open() {
                item("cold").openTrayMenu();
                const replacement = ({
                        token: Date.now() + 1
                    });
                tray.trayItems = [sample("cold", replacement, true)];
                tryVerify(() => menu("cold") !== null);
                compare(item("cold").trayMenuOpenRequested, false, "handle replacement cancels the old request");
                delayedRows = [
                    {
                        text: "Replacement rows",
                        isSeparator: false,
                        enabled: true
                    }
                ];
                tryCompare(menu("cold"), "hasMenuItems", true);
                wait(80);
                verify(!menu("cold").visible, "old native handle cannot satisfy a queued request");
                delayedRows = [];
                item("cold").openTrayMenu();
                compare(item("cold").trayMenuOpenRequested, true, "removal occurs with a real pending request");
                tray.trayItems = [];
                delayedRows = [
                    {
                        text: "Rows after removal",
                        isSeparator: false,
                        enabled: true
                    }
                ];
                wait(80);
                verify(item("cold") === null, "removed item and its request are destroyed");
            }
        }
    }
}
