import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 500; implicitHeight: 520
        color: Theme.background
        Item { id: host; anchors.fill: parent }
        EmojiPicker {
            id: picker
            hostItem: host
            shortcutEnabled: false
            copyOnSelect: false
            insertOnSelect: false
            persistRecent: false
        }
        TestCase {
            id: test
            property bool saved: false
            property string chosen: ""
            when: window.visible
            Connections { target: picker; function onChosen(emoji) { test.chosen = emoji; } }
            function test_picker() {
                report.setText("load\nFAILURES 1\n");
                tryVerify(() => picker.catalogue.length === 1914, 2000);
                report.setText("loaded " + picker.catalogue.length + "\nFAILURES 1\n");
                mouseMove(host, 1, 1);
                picker.open();
                const input = findChild(host, "emojiSearch");
                const grid = findChild(host, "emojiGrid");
                tryCompare(input, "activeFocus", true);
                compare(grid.count, picker.catalogue.length);
                verify(grid.contentItem.children.length < 100, "Emoji grid only creates visible cells");
                keyClick(Qt.Key_Tab);
                verify(findChild(host, "emojiSkinTone").activeFocus);
                report.setText("tone button\nFAILURES 1\n");
                keyClick(Qt.Key_Return);
                wait(100);
                keyClick(Qt.Key_Right);
                keyClick(Qt.Key_Right);
                keyClick(Qt.Key_Right);
                keyClick(Qt.Key_Return);
                report.setText("tone " + picker.skinTone + "\nFAILURES 1\n");
                compare(picker.skinTone, 3);
                tryCompare(input, "activeFocus", true);
                picker.query = "thumbs up";
                compare(picker.results.length, 1);
                compare(picker.results[0].emoji, "👍🏽");
                picker.query = "";
                keyClick(Qt.Key_Tab);
                keyClick(Qt.Key_Tab);
                verify(findChild(host, "emojiCategory0").activeFocus, "Tab focuses the selected category");
                keyClick(Qt.Key_Left);
                compare(picker.category, "Flags", "Category navigation wraps backwards");
                verify(findChild(host, "emojiCategory10").activeFocus);
                keyClick(Qt.Key_Right);
                compare(picker.category, "", "Category navigation wraps forwards");
                keyClick(Qt.Key_Right);
                compare(picker.category, "recent");
                keyClick(Qt.Key_Right);
                compare(picker.category, "Smileys & Emotion");
                verify(picker.results.every(row => row.group === picker.category));
                keyClick(Qt.Key_Down);
                verify(input.activeFocus, "Down returns to emoji navigation");
                compare(picker.selectedIndex, 0);
                keyClick(Qt.Key_Up);
                verify(findChild(host, "emojiCategory2").activeFocus, "Up from first emoji row reaches categories");
                keyClick(Qt.Key_Up);
                verify(input.activeFocus, "Up from categories returns to search");
                picker.category = "";
                keyClick(Qt.Key_Right);
                compare(picker.selectedIndex, 1);
                keyClick(Qt.Key_Down);
                compare(picker.selectedIndex, 9);
                keyClick(Qt.Key_Return);
                compare(picker.visible, true, "Selecting an emoji keeps the picker open");
                verify(chosen.length > 0);
                compare(picker.recent[0], chosen);
                keyClick(Qt.Key_Escape);
                compare(picker.visible, false, "Escape closes the picker after selection");
                picker.open();
                picker.category = "recent";
                compare(picker.results.length, 1);
                picker.category = "";
                picker.query = "thumbs up";
                verify(picker.results.length > 0);
                compare(picker.results[0].name, "thumbs up");
                picker.query = "no-emoji-with-this-name";
                compare(picker.results.length, 0);
                picker.choose();
                verify(picker.visible, "Empty results cannot select stale emoji");
                picker.query = "";
                wait(50);
                report.setText("scroll\nFAILURES 1\n");
                grid.contentY = 0;
                picker.moveSelection(56);
                const targetScroll = 8 * grid.cellHeight - grid.height;
                if (!Theme.reducedMotion) {
                    wait(45);
                    verify(grid.contentY > 0 && grid.contentY < targetScroll, "Keyboard scrolling interpolates instead of jumping");
                }
                tryVerify(() => Math.abs(grid.contentY - targetScroll) < 1, 400);
                picker.moveSelection(-56);
                wait(200);
                host.grabToImage(result => { test.saved = result.saveToFile("/tmp/bingux-emoji-picker.png"); });
                tryCompare(test, "saved", true, 2000);
                keyClick(Qt.Key_Escape);
                compare(picker.visible, false);
                report.setText("PASS Unicode catalogue, virtualised grid, keyboard selection, recents, search, empty results and Escape\nFAILURES 0\n");
            }
        }
    }
}
