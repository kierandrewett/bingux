import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: result
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 300
        implicitHeight: 500
        SidebarNotes {
            id: notes
            anchors.fill: parent
        }
        TestCase {
            when: window.visible
            function typeText(editor, text) {
                editor.forceActiveFocus();
                for (const character of text)
                    keyClick(character);
            }
            function test_markdown() {
                const editor = findChild(notes, "notesEditor");
                editor.clear();
                typeText(editor, "# ");
                typeText(editor, "Heading");
                compare(editor.getText(0, editor.length), "Heading");
                verify(editor.text.startsWith("# Heading"), editor.text);
                const headingHeight = editor.positionToRectangle(1).height;
                editor.text = "Body";
                const bodyHeight = editor.positionToRectangle(1).height;
                verify(headingHeight > bodyHeight, "Heading must render larger than body text");
                editor.text = notes.normaliseHeadings("# \\# Title");
                compare(editor.getText(0, editor.length), "Title");
                editor.cursorPosition = 2;
                editor.forceActiveFocus();
                compare(editor.activeSyntax, "#");
                const marker = findChild(editor, "notesSyntaxMarker");
                wait(150);
                verify(marker.opacity > 0 && marker.opacity < 1);
                window.contentItem.forceActiveFocus();
                wait(150);
                compare(marker.opacity, 0);
                compare(editor.getText(0, editor.length), "Title");
                editor.text = "# Title";
                editor.cursorPosition = 0;
                typeText(editor, "# ");
                compare(editor.getText(0, editor.length), "Title");
                editor.clear();
                typeText(editor, "Body");
                keyClick(Qt.Key_Return);
                typeText(editor, "## ");
                typeText(editor, "Second heading");
                verify(editor.text.includes("## Second heading"), editor.text);
                verify(editor.positionToRectangle(editor.length - 1).height > bodyHeight);
                keyClick(Qt.Key_Return);
                typeText(editor, "Normal paragraph");
                verify(editor.positionToRectangle(editor.length - 1).height <= bodyHeight, "Enter returns to body text");
                editor.clear();
                typeText(editor, "**Bold**");
                compare(editor.getText(0, editor.length), "Bold");
                verify(editor.text.includes("**Bold**"), editor.text);
                typeText(editor, " plain");

                verify(editor.text.includes("**Bold** plain"), editor.text);
                keyClick(Qt.Key_Z, Qt.ControlModifier);
                verify(editor.canRedo);
                editor.clear();
                typeText(editor, "- ");
                typeText(editor, "One");
                verify(editor.getText(0, editor.length).includes("One"));
                verify(!editor.getText(0, editor.length).includes("-"));
                verify(editor.text.includes("- One"), editor.text);
                keyClick(Qt.Key_Return);
                typeText(editor, "Two");

                verify(editor.text.includes("- Two"), editor.text);
                editor.clear();
                typeText(editor, "*italic*");
                compare(editor.getText(0, editor.length), "italic");
                editor.text = "# Saved\n\n**Existing** note";
                editor.cursorPosition = editor.length;
                typeText(editor, "!");
                verify(editor.text.includes("# Saved"));
                verify(editor.text.includes("**Existing**"));
                notes.save();
                editor.clear();
                typeText(editor, "`code`");
                compare(editor.getText(0, editor.length), "code");
                editor.clear();
                typeText(editor, "~~removed~~");
                compare(editor.getText(0, editor.length), "removed");
                editor.clear();
                typeText(editor, "```");
                keyClick(Qt.Key_Return);
                typeText(editor, "**literal**");
                compare(editor.getText(0, editor.length), "**literal**");
                editor.text = "Selected";
                editor.selectAll();
                keyClick(Qt.Key_B, Qt.ControlModifier);
                verify(editor.text.includes("**Selected**"), editor.text);
                editor.clear();
                typeText(editor, "[Qt](https://qt.io)");
                compare(editor.getText(0, editor.length), "Qt");
                verify(editor.text.includes("https://qt.io"), editor.text);
                const component = Qt.createComponent("SidebarNotes.qml");
                compare(component.status, Component.Ready);
                const restored = component.createObject(window.contentItem, {
                    width: 300,
                    height: 500
                });
                verify(restored !== null);
                compare(findChild(restored, "notesEditor").getText(0, editor.length), "Qt");
                restored.destroy();
            }
            function cleanupTestCase() {
                result.setText("FAILURES " + qtest_results.failCount + "\n");
                Qt.quit();
            }
        }
    }
}
