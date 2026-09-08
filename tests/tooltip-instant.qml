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

        }
        Item {
            id: controlAnchor
            x: 100; width: 80; height: 40
            MouseArea { id: controlHover; anchors.fill: parent; hoverEnabled: true }
            ShellTooltip { id: controlTip; visible: controlHover.containsMouse; text: "Control tooltip" }
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
            function test_session() {
                wait(100);
                mouseMove(window.contentItem, 220, 80);
                wait(1100);
                compare(Theme.tooltipsWarm, false);
                mouseMove(anchor, 20, 20);
                tryCompare(hover, "containsMouse", true);
                compare(barTip.testPopup.visible, false);
                wait(200);
                compare(barTip.testPopup.visible, false);
                tryCompare(barTip.testPopup, "visible", true, 600);
                compare(Theme.tooltipsWarm, true);
                wait(150);

                // Cross from a layer tooltip to a standard control tooltip.
                mouseMove(controlAnchor, 20, 20);
                tryCompare(controlHover, "containsMouse", true);
                compare(controlTip.visible, true);
                compare(controlTip.revealDuration, 0);
                compare(controlTip.delay, 0);
                mouseMove(window.contentItem, 220, 80);
                wait(100);
                dockTip.visible = true;
                const bubble = dockTip.contentItem.children.find(item => item.visible);
                compare(bubble.opacity, 1);
                dockTip.visible = false;
                wait(1100);
                compare(Theme.tooltipsWarm, false);

                // A new session restores the delay and initial fade.
                mouseMove(anchor, 20, 20);
                tryCompare(hover, "containsMouse", true);
                compare(barTip.testPopup.visible, false);
                tryCompare(barTip.testPopup, "visible", true, 700);
                mouseMove(window.contentItem, 220, 80);
                wait(1100);
                dockTip.visible = true;
                verify(bubble.opacity < 1);
                wait(160);
                compare(bubble.opacity, 1);
                dockTip.visible = false;
                console.warn("PASS: initial tooltip delay and fade, instant cross-surface switching, idle reset");
            }
        }
    }
}
