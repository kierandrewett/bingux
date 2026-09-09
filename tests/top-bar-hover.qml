import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    property string checks: ""
    FileView { id: results; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 560
        implicitHeight: 120
        color: Theme.shellSurface
        Pill {
            id: control
            x: 20; y: 20
            interactive: true
            hovered: pointer.containsMouse
            pressed: pointer.pressed
            Text { text: "Calendar"; color: Theme.text }
            MouseArea { id: pointer; parent: control; anchors.fill: parent; hoverEnabled: true }
        }
        Pill { id: passive; x: 20; y: 70; Text { text: "CPU 12%"; color: Theme.muted } }
        Tray { id: tray; x: 200; y: 20; parentWindow: window }
        QtObject {
            id: trayItem
            property string id: "hover-test"
            property string title: "Test"
            property string icon: "application-x-executable"
            property bool onlyMenu: false
            property bool hasMenu: false
            property var menu: null
            function activate() {}
        }
        QtObject {
            id: metrics
            property bool desktopStateAvailable: true
            property string inputSourceLabel: "en"
            property var currentInputSource: ({type: "xkb", id: "gb", displayName: "English"})
            property var inputSources: [currentInputSource]
        }
        InputSourceSelector {
            id: selector
            x: 300; y: 20
            parentWindow: window
            metrics: metrics
            gnoblinCtlPath: "true"
        }
        TestCase {
            name: "TopBarHover"
            when: window.visible
            function equal(actual, expected, message) {
                checks += message + ": " + actual + " (expected " + expected + ")\n";
                results.setText(checks);
                compare(actual, expected, message);
            }
            function surface(item) { return item.children[0]; }
            function away() { mouseMove(window.contentItem, 540, 100); wait(30); }
            function test_control() {
                wait(100);
                away();
                equal(surface(control).color.a, 0, "Resting control is transparent");
                mouseMove(control, control.width / 2, 0);
                equal(surface(control).color, Theme.hover, "Top edge hover matches dock");
                mousePress(control, control.width / 2, 0);
                equal(surface(control).color, Theme.pressed, "Press matches dock");
                mouseRelease(control, control.width / 2, 0);
                mouseMove(control, control.width / 2, control.height - 1);
                equal(surface(control).color, Theme.hover, "Bottom edge remains interactive");
                away();
                equal(surface(control).color.a, 0, "Leaving clears hover");
                control.selected = true;
                equal(surface(control).color, Theme.elevated, "Open menu retains active state");
                control.selected = false;
                control.forceActiveFocus();
                equal(surface(control).color, Theme.hover, "Keyboard focus matches dock");
                control.focus = false;
                mouseMove(passive, 10, 10);
                equal(surface(passive).color.a, 0, "Metrics do not suggest a click action");
            }
            function test_selector() {
                away();
                mouseMove(selector, 10, 0);
                equal(surface(selector).color, Theme.hover, "Keyboard selector hover before press");
                mousePress(selector, 10, 0);
                equal(surface(selector).color, Theme.pressed, "Keyboard selector press");
                mouseMove(window.contentItem, 540, 100);
                mouseRelease(window.contentItem, 540, 100);
                away();
                equal(surface(selector).color.a, 0, "Keyboard selector clears hover");
                metrics.desktopStateAvailable = false;
                mouseMove(selector, 10, 10);
                equal(surface(selector).color.a, 0, "Unavailable selector has no hover");
                metrics.desktopStateAvailable = true;
            }
            function test_tray() {
                tray.trayItems = [trayItem];
                wait(150);
                const button = findChild(tray, "trayItem-hover-test");
                tryCompare(button, "scale", 1);
                mouseMove(button, 16, 0);
                equal(surface(button).color, Theme.hover, "Tray hover before press");
                mousePress(button, 16, 0);
                equal(surface(button).color, Theme.pressed, "Tray press matches dock");
                mouseRelease(button, 16, 0);
                away();
                equal(surface(button).color.a, 0, "Tray clears hover");
                button.forceActiveFocus();
                equal(surface(button).color, Theme.hover, "Tray keyboard focus is visible");
            }
            function cleanupTestCase() {
                results.setText(checks + "FAILURES " + qtest_results.failCount);
            }
        }
    }
}
