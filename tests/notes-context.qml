import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: result
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }
    Timer {
        id: finish
        interval: 200
        onTriggered: Qt.quit()
    }
    Window {
        id: window
        visible: true
        flags: Qt.Window | Qt.WindowDoesNotAcceptFocus
        width: 420
        height: 640
        color: Theme.barBackground
        SidebarNotes {
            id: notes
            anchors.fill: parent
            menuHost: window.contentItem
        }
        TestCase {
            property string checks: ""
            function equal(a, b) {
                checks += "Compare " + a + " / " + b + "\n";
                compare(a, b);
            }
            function check(value, message) {
                checks += "Check " + value + " " + (message || "") + "\n";
                verify(value, message);
            }
            when: window.visible
            function test_menu_and_formatting() {
                const editor = findChild(notes, "notesEditor");
                editor.text = "Alpha beta";
                editor.select(0, 5);
                mouseClick(editor, 65, 10, Qt.RightButton);
                wait(250);
                const menu = findChild(notes, "notesContextMenu");
                check(menu !== null, "right click creates menu");
                check(menu.visible);
                equal(editor.selectedText, "Alpha");
                mouseClick(findChild(menu.body, "notesAction_bold"));
                wait(250);
                check(editor.text.includes("**Alpha**"), editor.text);
                check(!menu.visible);
                notes.applyAction("undo");
                check(!editor.text.includes("**Alpha**"));
                notes.applyAction("redo");
                check(editor.text.includes("**Alpha**"));
                for (const action of ["heading", "bullet", "numbered", "quote", "code", "plain"]) {
                    editor.text = "Before\n\nTarget\n\nAfter";
                    const plain = editor.getText(0, editor.length);
                    editor.cursorPosition = plain.indexOf("Target") + 2;
                    notes.applyAction(action);
                    equal(editor.getText(0, editor.length), plain);
                    check(editor.text.includes("Before") && editor.text.includes("After"), action);
                    if (action === "heading")
                        check(editor.text.includes("## Target"), editor.text);
                }
                editor.text = "# Title\n\nA paragraph.";
                editor.cursorPosition = 2;
                notes.applyAction("plain");
                check(!editor.text.startsWith("#"), editor.text);
                editor.text = "# Title\n\nA paragraph.";
                editor.cursorPosition = 2;
                editor.forceActiveFocus();
                keyClick(Qt.Key_F10, Qt.ShiftModifier);
                wait(250);
                check(menu.visible, "keyboard context menu");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT"))
                    grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT"));
                keyClick(Qt.Key_Escape);
                wait(200);
                check(!menu.visible, "Escape dismisses menu");
                editor.text = "Title";
                editor.cursorPosition = 2;
                notes.showContextMenu(Qt.point(30, 30));
                wait(200);
                mouseClick(findChild(menu.body, "notesAction_headings"));
                wait(100);
                const fourthHeading = findChild(menu.body, "notesAction_heading4");
                check(fourthHeading !== null, "heading submenu contains six levels");
                mouseClick(fourthHeading);
                wait(150);
                check(editor.text.includes("#### Title"), "heading menu sets the requested level");
                if (Quickshell.env("BINGUX_NOTES_SCREENSHOT"))
                    grabImage(window.contentItem).save(Quickshell.env("BINGUX_NOTES_SCREENSHOT") + ".editing.png");
            }
            function cleanupTestCase() {
                result.setText("FAILURES " + qtest_results.failCount + "\n" + checks);
                finish.start();
            }
        }
    }
}
