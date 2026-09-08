import QtQuick
import QtTest
import Quickshell

ShellRoot {
    DockTooltip { id: dockTip; text: "Dock tooltip" }
    FloatingWindow {
        id: window
        implicitWidth: 240
        implicitHeight: 100
        Item {
            id: anchor
            width: 80; height: 40
            MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true }
            ShellTooltip { id: controlTip; visible: hover.containsMouse; text: "Control tooltip" }
        }
        BarTooltip {
            id: barTip
            anchorItem: anchor
            barWindow: window
            requested: hover.containsMouse
            text: "Top bar tooltip"
        }
        TestCase {
            name: "InstantTooltips"
            when: window.visible
            function test_dock() {
                dockTip.visible = true;
                const bubble = dockTip.contentItem.children.find(item => item.visible);
                compare(bubble.opacity, 1);
                dockTip.visible = false;
            }
            function test_hover() {
                wait(100);
                for (let i = 0; i < 3; ++i) {
                    mouseMove(window.contentItem, 200, 80);
                    tryCompare(hover, "containsMouse", false);
                    compare(barTip.testPopup.visible, false);
                    mouseMove(anchor, 20, 20);
                    tryCompare(hover, "containsMouse", true);
                    // No wait for either tooltip after the hover event.
                    compare(barTip.testPopup.visible, true);
                    compare(controlTip.visible, true);
                    compare(controlTip.delay, 0);
                }
                console.warn("PASS: top bar and control tooltips show on the hover event");
            }
        }
    }
}
