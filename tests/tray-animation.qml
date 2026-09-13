import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }

    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 500
        implicitHeight: 100
        color: "black"

        Tray {
            id: tray
            parentWindow: window
            serviceEnabled: false
        }

        TestCase {
            when: window.visible

            function initTestCase() {
                report.setText("RUNNING");
            }

            function cleanupTestCase() {
                report.setText("FAILURES " + qtest_results.failCount);
            }

            function item(id) {
                return findChild(tray, "trayItem-" + id);
            }

            function sample(id) {
                return {
                    id,
                    title: id,
                    tooltipTitle: id,
                    icon: "application-x-executable",
                    menu: null,
                    hasMenu: false,
                    onlyMenu: false,
                    activate: () => {},
                    secondaryActivate: () => {},
                    scroll: () => {}
                };
            }

            function test_new_item_does_not_reanimate_existing_items() {
                tray.trayItems = [sample("one"), sample("two")];
                tryVerify(() => item("one") && item("two"), 1000);
                tryVerify(() => item("one").opacity > 0.99 && item("one").scale > 0.99 && item("two").opacity > 0.99 && item("two").scale > 0.99, 1000);
                wait(500);
                const first = item("one");
                const second = item("two");

                tray.trayItems = [sample("one"), sample("two"), sample("three")];
                tryVerify(() => item("three") && item("three").slideOffset > 0.5, 250);
                compare(item("one"), first, "Existing item one keeps its delegate");
                compare(item("two"), second, "Existing item two keeps its delegate");
                verify(item("one").opacity > 0.99 && Math.abs(item("one").slideOffset) < 0.1, "Existing item one remains settled");
                verify(item("two").opacity > 0.99 && Math.abs(item("two").slideOffset) < 0.1, "Existing item two remains settled");
            }

            function test_empty_tray_menu_stays_hidden() {
                const empty = sample("empty-menu");
                empty.hasMenu = true;
                empty.onlyMenu = true;
                tray.trayItems = [empty];
                tryVerify(() => item("empty-menu") !== null, 1000);
                const menu = findChild(item("empty-menu"), "trayItemMenu");
                verify(!menu.hasMenuItems, "A menu-less tray item has no menu rows");
                menu.open();
                wait(50);
                verify(!menu.visible, "A menu-less tray item does not show an empty popup");
            }
        }
    }
}
