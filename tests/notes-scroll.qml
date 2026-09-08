import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 100; onTriggered: Qt.quit() }
    Window {
        id: window
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        width: 340; height: 360
        color: Theme.barBackground
        SidebarNotes { id: notes; anchors.fill: parent }
        TestCase {
            id: test
            when: window.visible
            property var editor: findChild(notes, "notesEditor")
            property bool watching: false
            property real minimumHeight: 0
            property string observations: ""
            Connections {
                target: test.editor
                function onContentHeightChanged() {
                    if (test.watching) test.minimumHeight = Math.min(test.minimumHeight, test.editor.contentHeight);
                }
            }
            function caretY() {
                const rect = editor.cursorRectangle;
                return editor.mapToItem(notes, rect.x, rect.y).y;
            }
            function type(text) { for (const ch of text) keyClick(ch); wait(25); }
            function prepare() {
                // Leave enough text on both sides of a visible middle paragraph.
                editor.text = "# First title\n\n" + Array.from({length: 24}, (_, i) => "Line " + i).join("\n\n");
                editor.forceActiveFocus();
                editor.cursorPosition = editor.length;
                wait(50);
                editor.cursorPosition = editor.getText(0, editor.length).indexOf("Line 15");
                wait(50);
            }
            function test_heading_keeps_viewport() {
                for (const action of ["slash", "markdown", "context"]) {
                    prepare();
                    if (action === "slash") type("/h1");
                    else if (action === "markdown") type("##");
                    const beforeY = caretY(), beforeHeight = editor.contentHeight;
                    minimumHeight = beforeHeight;
                    watching = true;
                    if (action === "slash") keyClick(Qt.Key_Return);
                    else if (action === "markdown") keyClick(Qt.Key_Space);
                    else notes.applyAction("heading2");
                    wait(50);
                    watching = false;
                    const jump = Math.abs(caretY() - beforeY);
                    observations += action + ": minimum height " + minimumHeight + ", original height " + beforeHeight + ", caret movement " + jump + "\n";
                    verify(minimumHeight >= beforeHeight - 1, action + " exposes the intermediate shortened document");
                    verify(jump <= 16, action + " moves the viewport beyond the heading's 14 px top margin");
                    const settledY = caretY();
                    type("Title");
                    compare(caretY(), settledY, "Typing an unwrapped heading must not move vertically");
                    const beforeEnterY = caretY();
                    keyClick(Qt.Key_Return);
                    wait(50);
                    verify(caretY() >= beforeEnterY && caretY() - beforeEnterY < 90, "Enter must advance to the next paragraph without a scroll jump");
                    verify(editor.text.includes("# First title") && editor.text.includes("Line 23"), "Neighbouring content survives");
                }
            }
            function cleanupTestCase() {
                report.setText("FAILURES " + qtest_results.failCount + "\n" + observations);
                finish.start();
            }
        }
    }
}
