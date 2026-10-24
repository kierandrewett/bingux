import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    id: overlay
    visible: true
    WlrLayershell.namespace: "bingux-command-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    TestCase {
        parent: overlay.contentItem
        name: "CommandMode"
        when: overlay.visible
        function equal(actual, expected, label) {
            console.warn(label + ': ' + actual + ' expected ' + expected);
            compare(actual, expected, label);
        }
        function test_command_mode() {
            const input = findChild(overlay.contentItem, 'searchInput');
            const socket = findChild(overlay, 'searchSocket');
            input.forceActiveFocus();
            input.text = '>echo "hello world"';
            equal(overlay.commandMode, true, 'Leading > enters command mode');
            equal(overlay.queryForSearch(), '>echo "hello world"', 'Command text is sent unchanged');
            equal(socket.queries[socket.queries.length - 1], '>echo "hello world"', 'Daemon receives the command query');
            socket.resultsReceived(overlay.activeRequestId, [{
                resultId: 'command-1',
                providerId: 'command',
                kind: 'command',
                title: 'echo "hello world"',
                subtitle: 'Run command',
                icon: 'utilities-terminal',
                score: 1
            }], true);
            equal(overlay.displayedResults.length, 1, 'One command candidate is shown');
            equal(overlay.selectedIndex, 0, 'Command candidate is selected');
            keyClick(Qt.Key_Return);
            tryVerify(function () { return socket.activations.length === 1; }, 5000, 'Enter activates once after launch feedback dispatch');
            equal(socket.activations[0], 'command-1', 'Enter activates the selected command');
            equal(overlay.activationPending, true, 'Activation is pending until the daemon replies');
            socket.requestFailed(overlay.activeActivationRequestId, 'spawn-failed');
            equal(overlay.queryError, 'Could not run echo "hello world".', 'Failure is shown in the surface');
            equal(overlay.activationPending, false, 'Failure clears the pending launch');
            keyClick(Qt.Key_Escape);
            equal(input.text, '', 'Escape clears the command text');
            equal(overlay.closing, false, 'First Escape keeps the surface open');
            keyClick(Qt.Key_Escape);
            equal(overlay.closing, true, 'Escape on an empty command surface closes it');
            console.warn('SEARCH_COMMAND_PASS');
        }
        function cleanupTestCase() {
            wait(200);
            Qt.quit();
        }
    }
}
