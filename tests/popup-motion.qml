import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    property string checks: ""
    FileView { id: results; path: Quickshell.env("BINGUX_POPUP_TEST_RESULTS") }
    ShellPopup {
        id: popup
        popupWidth: 240
        popupHeight: 140
        Text { text: "Shared context menu"; color: Theme.text }
    }
    TestCase {
        parent: popup.contentItem
        name: "PopupMotion"
        when: true
        function check(value, message) {
            checks += message + ": " + value + "\n";
            results.setText(checks);
            verify(value, message);
        }
        function card() { return popup.body.parent; }
        function mapped() {
            const window = popup.contentItem.Window.window;
            return window !== null && window.visible;
        }
        function openSettled() {
            popup.visible = true;
            tryVerify(() => popup.revealScale === 1 && card().opacity === 1, 1000);
            waitForRendering(card());
            check(mapped(), "Open menu is mapped");
            check(card().opacity === 1 && popup.revealScale === 1, "Open menu settles at full opacity and scale");
        }
        function finishClose() {
            wait(Theme.popupCloseMotion + 80);
            check(card().opacity === 0, "Closing reaches zero opacity");
            check(!mapped(), "Transparent window is unmapped");
        }
        function cleanup() {
            popup.visible = false;
            wait(Theme.popupCloseMotion + 80);
        }
        function test_close() {
            openSettled();
            popup.visible = false;
            check(!popup.contentItem.enabled, "Closing immediately disables menu input");
            if (!Theme.reducedMotion) {
                wait(40);
                check(mapped() && card().opacity > 0 && card().opacity < 1,
                    "Closing keeps the card mapped during the fade");
                check(popup.revealScale === 1, "Closing never changes scale");
            }
            finishClose();
        }
        function test_open() {
            popup.visible = true;
            if (!Theme.reducedMotion) {
                wait(40);
                check(popup.revealScale > Theme.popupInitialScale && popup.revealScale < 1,
                    "Opening scales towards full size");
                check(card().opacity > 0 && card().opacity < 1, "Opening fades in");
            }
            wait(Theme.popupOpenMotion + 80);
            check(popup.revealScale === 1 && card().opacity === 1, "Opening reaches its final state");
        }
        function test_interruptedOpen() {
            if (Theme.reducedMotion) return;
            popup.visible = true;
            wait(40);
            const scale = popup.revealScale;
            const opacity = card().opacity;
            popup.visible = false;
            wait(35);
            check(popup.revealScale === scale, "Closing an opening menu freezes its current scale");
            check(card().opacity < opacity && card().opacity > 0, "Interrupted opening fades from its current opacity");
            finishClose();
        }
        function test_reopen() {
            openSettled();
            popup.visible = false;
            if (!Theme.reducedMotion) wait(40);
            const opacity = card().opacity;
            popup.visible = true;
            if (!Theme.reducedMotion) {
                check(card().opacity === opacity, "Reopening does not reset opacity");
                check(popup.revealScale === 1, "Reopening does not restart the scale effect");
            }
            wait(Theme.popupOpenMotion + Theme.popupCloseMotion + 80);
            check(mapped() && popup.visible && card().opacity === 1, "A cancelled close cannot hide the reopened menu");
        }
        function test_outsideClick() {
            openSettled();
            mouseClick(popup.testDismissContentItem, 2, 2);
            check(!popup.visible, "Outside click requests close");
            finishClose();
        }
        function test_escape() {
            openSettled();
            // Focus the real window with an inside click, then send Escape.
            mouseClick(card(), card().width / 2, card().height / 2);
            popup.contentItem.forceActiveFocus();
            keyClick(Qt.Key_Escape);
            check(!popup.visible, "Escape requests close");
            finishClose();
        }
        function cleanupTestCase() {
            results.setText(checks + "FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }
    }
}
