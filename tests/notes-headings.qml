import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    FloatingWindow {
        id: window
        implicitWidth: 420; implicitHeight: 700
        color: Theme.barBackground
        SidebarNotes { id: notes; anchors.fill: parent }
        TestCase {
            property string checks: ""
            function check(value, message) { checks += "Check " + value + " " + (message || "") + "\n"; verify(value,message); }
            when: window.visible
            function type(editor, text) { editor.forceActiveFocus(); for (const c of text) keyClick(c); }
            function test_consecutive_headings() {
                const editor = findChild(notes, "notesEditor");
                editor.clear();
                const positions = [], sizes = [];
                for (let level = 1; level <= 6; level++) {
                    type(editor, "#".repeat(level) + " Heading " + level);
                    check(editor.text.includes("#".repeat(level) + " Heading " + level), "heading " + level + " keeps its Markdown level");
                    const index = editor.getText(0, editor.length).indexOf("Heading " + level);
                    positions.push(index);
                    sizes.push(editor.positionToRectangle(index).height);
                    for (let n = 0; n < positions.length; n++) {
                        check(editor.positionToRectangle(positions[n]).height === sizes[n], "earlier heading " + (n + 1) + " size unchanged");
                        check(editor.text.split("\n").includes("#".repeat(n + 1) + " Heading " + (n + 1)), "earlier heading semantic level preserved");
                    }
                    keyClick(Qt.Key_Return);
                }
                type(editor, "A normal paragraph.");
                check(!editor.getText(0, editor.length).includes("#"), "no literal markers left in rendered headings");
                check(sizes[0] > sizes[1] && sizes[1] > sizes[2], "heading hierarchy is distinct");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT")) grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT") + ".levels.png");
                editor.text = "# Earlier\n\n**Bold** paragraph\n\n## Change me\n\nAfter";
                editor.cursorPosition = editor.getText(0, editor.length).indexOf("Change me");
                type(editor, "#### ");
                check(editor.text.includes("#### Change me"), "typed prefix changes the active heading level");
                check(editor.text.includes("# Earlier") && editor.text.includes("**Bold** paragraph"), "neighbouring formatting survives");
                const before = editor.text;
                editor.cursorPosition = editor.getText(0, editor.length).indexOf("After");
                notes.applyAction("heading3");
                const after = editor.text;
                keyClick(Qt.Key_Z, Qt.ControlModifier);
                check(editor.text === before, "one undo restores the complete formatting action");
                keyClick(Qt.Key_Z, Qt.ControlModifier | Qt.ShiftModifier);
                check(editor.text === after, "one redo restores the formatting action");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT")) grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT"));
            }
            function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n" + checks); finish.start(); }
        }
    }
}
