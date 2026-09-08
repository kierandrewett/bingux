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
            function equal(actual, expected, message) {
                if (actual !== expected) console.error("FAIL: " + message + " actual=" + actual + " expected=" + expected);
                compare(actual, expected, message);
            }
            function check(condition, message) {
                if (!condition) console.error("FAIL: " + message);
                verify(condition, message);
            }
            function test_session() {
                wait(100);
                mouseMove(window.contentItem, 220, 80);
                wait(1250);
                equal(Theme.tooltipDelay, 500);
                mouseMove(anchor, 20, 20);
                tryCompare(hover, "containsMouse", true);
                equal(barTip.testPopup.visible, false);
                wait(200);
                equal(barTip.testPopup.visible, false);
                tryCompare(barTip.testPopup, "visible", true, 600);
                wait(150);

                // Cross from a layer tooltip to a standard control tooltip.
                mouseMove(controlAnchor, 20, 20);
                tryCompare(controlHover, "containsMouse", true);
                equal(controlTip.visible, true);
                equal(controlTip.revealDuration, Theme.tooltipMotion);
                equal(controlTip.delay, 0);
                check(controlTip.opacity < 1, "Immediate control tooltip still fades in");
                check(controlTip.scale < 1, "Immediate control tooltip still scales in");
                const barBubble = barTip.testPopup.contentItem.children[0];
                check(barTip.testPopup.visible, "Bar window remains mapped during its exit");
                wait(40);
                check(barBubble.opacity > 0 && barBubble.opacity < 1, "Bar tooltip fades out");
                tryCompare(controlTip, "visible", true, 700);
                wait(200);
                mouseMove(window.contentItem, 220, 80);
                wait(100);
                dockTip.shown = true;
                const bubble = dockTip.contentItem.children.find(item => item.visible);
                wait(200);
                tryCompare(bubble, "opacity", 1, 400);
                dockTip.text = "Next dock target";
                tryCompare(dockTip, "presentedText", "Next dock target");
                check(bubble.opacity < 1, "Dock target switches keep the fade animation");
                check(bubble.scale < 1, "Dock target switches keep the scale animation");
                wait(200);
                dockTip.shown = false;
                equal(dockTip.visible, true, "Dock window stays mapped for the exit");
                wait(40);
                check(bubble.opacity > 0 && bubble.opacity < 1, "Dock tooltip fades out");
                check(bubble.scale < 1, "Dock tooltip scales during exit");
                dockTip.shown = true;
                wait(200);
                tryCompare(bubble, "opacity", 1, 400);
                tryCompare(bubble, "scale", 1, 400);
                dockTip.shown = false;
                tryCompare(dockTip, "visible", false, 300);
                wait(1250);
                equal(Theme.tooltipDelay, 500);

                // A new session restores the delay and initial fade.
                mouseMove(anchor, 20, 20);
                tryCompare(hover, "containsMouse", true);
                equal(barTip.testPopup.visible, false);
                tryCompare(barTip.testPopup, "visible", true, 700);
                mouseMove(window.contentItem, 220, 80);
                wait(1250);
                dockTip.shown = true;
                check(bubble.opacity < 1);
                check(bubble.scale < 1, "First reveal starts scaled down");
                wait(200);
                tryCompare(bubble, "opacity", 1, 400);
                tryCompare(bubble, "scale", 1, 400);
                dockTip.shown = false;
                console.warn("PASS: first hover waits, switching skips delay but keeps motion, animated exit and re-entry");
            }
        }
    }
}
