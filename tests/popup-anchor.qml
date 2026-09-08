import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

ShellRoot {
    PanelWindow {
        id: bar
        anchors { top: true; left: true; right: true }
        implicitHeight: 32
        margins.right: 300
        exclusionMode: ExclusionMode.Ignore
        Item {
            id: group
            x: bar.width - 160
            width: 140
            height: 32
            Rectangle { id: button; x: 70; width: 50; height: 32; color: "steelblue" }
        }
    }
    ShellPopup {
        id: popup
        anchorWindow: bar
        anchorItem: button
        screen: bar.screen
        popupWidth: 260
        popupHeight: 120
    }
    ShellPopup { id: overflow; screen: bar.screen; preferredX: 100; preferredY: 100; popupWidth: 200; popupHeight: 100 }
    TestCase {
        parent: popup.contentItem
        when: true
        function checkAlignment() {
            const point = button.mapToItem(bar.contentItem, button.width, button.height);
            console.info("ANCHOR", popup.preferredX, bar.margins.left + point.x - popup.popupWidth,
                popup.preferredY, popup.panelX, popup.body.parent.width, popup.width, bar.margins.right);
            tryCompare(popup, "preferredX", bar.margins.left + point.x - popup.popupWidth);
            compare(popup.preferredY, 40);
            verify(popup.panelX >= bar.margins.left + Theme.gap);
            verify(popup.panelX + popup.body.parent.width <= popup.width - bar.margins.right - Theme.gap);
        }
        function test_anchor() {
            popup.visible = true;
            tryVerify(() => bar.width > 600 && popup.width > 600);
            waitForRendering(popup.body);
            checkAlignment();
            for (const inset of [0, 420, 300]) {
                bar.margins.right = inset;
                wait(100);
                checkAlignment();
            }
            bar.margins.right = 0;
            bar.margins.left = 300;
            wait(100);
            checkAlignment();
            group.x -= 40;
            wait(20);
            checkAlignment();
            popup.anchorAlignment = Qt.AlignHCenter;
            compare(popup.preferredX, bar.margins.left + group.x + button.x + button.width / 2 - popup.popupWidth / 2);
            popup.anchorAlignment = Qt.AlignRight;
            popup.popupWidth = 340;
            checkAlignment();
            // The left boundary also constrains a centred popup near the edge.
            group.x = 0;
            tryCompare(popup, "panelX", bar.margins.left + Theme.gap);
            // A control moved into overflow uses its new scene, without adding
            // the bar's left margin a second time.
            overflow.visible = true;
            tryCompare(overflow, "revealScale", 1);
            button.parent = overflow.body;
            wait(100);
            console.info("OVERFLOW", popup.anchorPosition.x, overflow.panelX + overflow.contentPadding + button.x + button.width,
                popup.preferredY, overflow.panelY + overflow.contentPadding + button.height + Theme.gap);
            compare(popup.anchorPosition.x, overflow.panelX + overflow.contentPadding + button.x + button.width);
            compare(popup.preferredY, overflow.panelY + overflow.contentPadding + button.height + Theme.gap);
            popup.visible = false;
            overflow.visible = false;
            tryVerify(() => !popup.retained && !overflow.retained);
            console.info("POPUP_ANCHOR_PASSED");
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
