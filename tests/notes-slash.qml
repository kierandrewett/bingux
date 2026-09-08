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
        SidebarNotes { id: notes; anchors.top: parent.top; anchors.bottom: parent.bottom; width: parent.width }
        TestCase {
            when: window.visible
            property var editor
            property var menu
            property string checks: ""
            function equal(a, b) { checks += "Compare: " + a + " / " + b + "\n"; compare(a, b); }
            function check(value, message) { checks += message + ": " + value + "\n"; verify(value, message); }
            function type(text) { editor.forceActiveFocus(); for (const c of text) keyClick(c); wait(20); }
            function init() {
                editor = findChild(notes, "notesEditor");
                menu = findChild(notes, "notesSlashMenu");
                menu.close();
                editor.text = "";
                editor.continuation = null;
                editor.forceActiveFocus();
            }
            function test_navigation() {
                type("/");
                check(menu.visible, "slash opens menu");
                check(editor.activeFocus, "editor retains typing focus");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT")) grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT") + ".all.png");
                equal(menu.selectedCommand.id, "plain");
                keyClick(Qt.Key_Down);
                equal(menu.selectedCommand.id, "heading1");
                keyClick(Qt.Key_Up);
                equal(menu.selectedCommand.id, "plain");
                type("h2");
                equal(menu.selectedCommand.id, "heading2");
                keyClick(Qt.Key_Return);
                type("Title");
                check(editor.text.includes("## Title"), "Enter inserts heading");
                check(!editor.getText(0, editor.length).includes("/h2"), "command text removed");
                keyClick(Qt.Key_Return);
                type("Body");
                check(!editor.text.includes("## Body"), "Enter exits heading");
            }
            function test_dismissal() {
                type("/notacommand");
                check(menu.visible && menu.matches.length === 0, "empty results stay visible");
                const before = editor.text;
                keyClick(Qt.Key_Return);
                equal(editor.text, before);
                keyClick(Qt.Key_Escape);
                check(!menu.visible, "Escape dismisses");
                equal(editor.text, before);
                type(" literal");
                check(!menu.visible, "dismissed slash stays literal");
                editor.text = "";
                type("https://example.com/path");
                check(!menu.visible, "URL slashes do not trigger");
                editor.text = "";
                type("/h1");
                keyClick(Qt.Key_Backspace);
                keyClick(Qt.Key_Backspace);
                check(menu.visible, "backspace resets query");
                keyClick(Qt.Key_Backspace);
                wait(20);
                check(!menu.visible, "deleting slash dismisses");
                editor.text = "```\ncode\n```";
                editor.cursorPosition = editor.length;
                type(" /");
                check(!menu.visible, "code remains literal");
            }
            function test_inline_and_undo() {
                for (const [query, expected] of [["bold", "**Words**"], ["italic", "*Words*"], ["strike", "~~Words~~"], ["inline code", "`Words`"]]) {
                    editor.text = "# Before\n\nHere tail\n\nAfter";
                    editor.cursorPosition = editor.getText(0, editor.length).indexOf("Here") + 5;
                    type("/" + query);
                    const before = editor.text;
                    keyClick(Qt.Key_Return);
                    const inserted = editor.text;
                    check(editor.selectedText.length > 0, query + " placeholder selected");
                    keyClick(Qt.Key_Z, Qt.ControlModifier);
                    equal(editor.text, before);
                    keyClick(Qt.Key_Z, Qt.ControlModifier | Qt.ShiftModifier);
                    equal(editor.text, inserted);
                    // Repeat to check typing replaces the selected placeholder.
                    keyClick(Qt.Key_Z, Qt.ControlModifier);
                    editor.cursorPosition = editor.getText(0, editor.length).indexOf("/" + query) + query.length + 1;
                    editor.remove(editor.cursorPosition - query.length - 1, editor.cursorPosition);
                    type("/" + query);
                    keyClick(Qt.Key_Return);
                    type("Words");
                    check(editor.text.includes(expected), query + " has semantic Markdown: " + editor.text);
                    check(editor.text.includes("# Before") && editor.text.includes("After"), "neighbours preserved");
                }
            }
            function test_blocks_and_table() {
                for (const [query, expected] of [["bullet", "- Item"], ["num", "1.  Item"], ["todo", "[ ] Item"], ["quote", "> Item"], ["code", "```"]]) {
                    editor.text = "";
                    type("/" + query);
                    keyClick(Qt.Key_Return);
                    type("Item");
                    check(editor.text.includes(expected), query + " inserted: " + editor.text);
                }
                editor.text = "# Before\n\n\n\nAfter";
                editor.cursorPosition = editor.getText(0, editor.length).indexOf("After");
                type("/table");
                const before = editor.text;
                keyClick(Qt.Key_Return);
                check(editor.text.includes("|Column 1") && editor.text.includes("Column 2"), "table stored as Markdown: " + editor.text);
                equal(editor.selectedText, "Column 1");
                type("Name");
                check(editor.text.includes("Name") && !editor.text.includes("Column 1"), "table cell editable");
                check(editor.text.includes("# Before") && editor.text.includes("After"), "table preserves neighbours");
                notes.save();
                const restored = Qt.createComponent("SidebarNotes.qml").createObject(window.contentItem, {width: 320, height: 600});
                equal(findChild(restored, "notesEditor").text, editor.text);
                restored.destroy();
            }
            function test_extra_commands() {
                type("Hello /duplicate");
                keyClick(Qt.Key_Return);
                check(editor.getText(0, editor.length).split("Hello").length === 3, "duplicate copies current block: " + editor.text);
                editor.text = "Before\n\nDelete me \n\nAfter";
                editor.cursorPosition = editor.getText(0, editor.length).indexOf("Delete me") + 9;
                type(" /delete");
                const before = editor.text;
                keyClick(Qt.Key_Return);
                check(!editor.text.includes("Delete me") && editor.text.includes("Before") && editor.text.includes("After"), "delete removes only current block");
                keyClick(Qt.Key_Z, Qt.ControlModifier);
                equal(editor.text, before);
                editor.text = "";
                type("/div");
                keyClick(Qt.Key_Return);
                type("Below");
                check(/^[-* ]{3,}$/m.test(editor.text) && editor.text.includes("Below"), "divider permits following text: " + editor.text);
                editor.text = "";
                type("/date");
                keyClick(Qt.Key_Return);
                check(editor.text.includes(Qt.formatDate(new Date(), "yyyy-MM-dd")), "date inserts current local date");
                editor.text = "";
                type("/link");
                keyClick(Qt.Key_Return);
                equal(editor.selectedText, "https://example.com");
                type("https://qt.io");
                keyClick(Qt.Key_Return);
                check(editor.text.includes("[Link text](https://qt.io)"), "link URL is editable and Enter formats it");
                equal(editor.getText(0, editor.length), "Link text");
                editor.text = "";
                type("/image");
                keyClick(Qt.Key_Return);
                equal(editor.selectedText, "file:///path/to/image.png");
                const imageUrl = "file://" + Quickshell.env("BINGUX_NOTES_TEST_RESULTS") + ".png";
                grabImage(editor).save(imageUrl.slice(7));
                type(imageUrl);
                keyClick(Qt.Key_Return);
                check(editor.text.includes("![Image description](" + imageUrl + ")"), "image produces Markdown image");
            }
            function test_mouse_and_bounds() {
                type("/");
                wait(100);
                check(menu.x >= 0 && menu.y >= 0 && menu.y + menu.height <= notes.height, "menu stays inside notes");
                const button = findChild(menu.contentItem, "notesCommand_heading2");
                check(button !== null, "heading pointer target exists");
                mouseClick(button);
                check(!menu.visible, "click inserts and closes");
                type("Clicked");
                check(editor.text.includes("## Clicked"), "pointer-selected heading is editable");
                editor.text = Array(45).fill("Line").join("\n\n");
                editor.cursorPosition = editor.length;
                type(" /tab");
                wait(150);
                check(menu.visible, "menu opens in scrolled note");
                check(menu.y >= 0 && menu.y + menu.height <= notes.height, "scrolled menu fits");
                notes.width = 300;
                wait(100);
                check(menu.width <= 284 && menu.x + menu.width <= notes.width, "menu fits narrow sidebar");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT")) grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT"));
            }
            function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n" + checks); finish.start(); }
        }
    }
}
