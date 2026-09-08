import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    quickChatEnabled: true
    id: overlay
    visible: true
    WlrLayershell.namespace: "bingux-ai-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    TestCase {
        parent: overlay.contentItem
        name: "StreamingAI"
        when: overlay.visible
        function equal(actual, expected, label) { console.warn(label + ': ' + actual + ' expected ' + expected); compare(actual, expected, label); }
        function test_stream() {
            const input = findChild(overlay.contentItem, 'searchInput');
            const socket = findChild(overlay, 'searchSocket');
            input.text = '! Explain streaming';
            equal(overlay.chatPrompt(), 'Explain streaming', 'Prefix removed');
            overlay.activateChatResult({resultId: 'ai-1'});
            equal(overlay.chatPending, true, 'Activation waits for streamed answer');
            const request = overlay.activeActivationRequestId;
            socket.chatProgress('stale', 'Wrong response');
            equal(overlay.streamingChatText, '', 'Stale updates ignored');
            socket.chatProgress(request, 'First line\n');
            equal(overlay.streamingChatText, 'First line\n', 'Text appears before completion');
            equal(overlay.chatPending, true, 'Streaming keeps request open');
            socket.chatProgress(request, 'First line\nSecond line');
            wait(80);
            socket.chatReceived(request, 'First line\nSecond line');
            equal(overlay.chatTranscript.length, 1, 'No duplicate streamed/final messages');
            equal(overlay.chatTranscript[0].message, 'First line\nSecond line', 'Final multiline answer');
            equal(overlay.chatPending, false, 'Completion clears busy state');
            input.text = 'Follow-up';
            equal(overlay.queryForSearch(), '!Follow-up', 'Follow-ups stay in AI mode');
            overlay.activateChatResult({resultId: 'ai-2'});
            const cancelled = overlay.activeActivationRequestId;
            overlay.closeSearch();
            verify(socket.cancelled.includes(cancelled));
            console.warn('SEARCH_AI_PASS');
        }
        function cleanupTestCase() { wait(200); Qt.quit(); }
    }
}
