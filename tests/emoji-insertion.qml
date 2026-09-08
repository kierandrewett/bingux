import QtQuick
import QtQuick.Controls
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    EmojiPicker { id: picker; shortcutEnabled: false; persistRecent: false }
    FloatingWindow {
        id: window
        title: "Bingux emoji insertion test"
        implicitWidth: 440; implicitHeight: 120
        TextField { id: receiver; anchors.fill: parent; focus: true; text: "Before " }
        TestCase {
            when: window.visible
            function test_insertion() {
                report.setText("waiting for controlled receiver focus\nFAILURES 1\n");
                waitForRendering(receiver);
                mouseClick(receiver);
                tryCompare(picker, "lastFocusedTitle", window.title, 3000);
                tryVerify(() => picker.catalogue.length === 1914, 2000);
                for (const name of ["grinning face", "woman technologist", "thumbs up"]) {
                    picker.skinTone = name === "thumbs up" ? 3 : 0;
                    const before = receiver.text;
                    receiver.cursorPosition = receiver.text.length;
                    picker.open();
                    picker.query = name;
                    tryVerify(() => picker.selectedEmoji !== null, 1000);
                    compare(picker.selectedEmoji.name, name);
                    const emoji = picker.selectedEmoji.emoji;
                    wait(100);
                    report.setText("inserting " + name + "\nFAILURES 1\n");
                    // This deliberately drives the real native insertion path,
                    // but only after the controlled receiver was proven focused.
                    picker.choose();
                    wait(300);
                    report.setText(JSON.stringify({text: receiver.text, error: picker.error, pending: picker.pendingText, target: picker.targetWindow}) + "\nFAILURES 1\n");
                    tryCompare(receiver, "text", before + emoji, 2500);
                    compare(picker.error, "");
                    tryCompare(picker, "pendingText", "", 1000);
                    wait(100);
                }
                report.setText("PASS actual app insertion: plain emoji, ZWJ sequence, skin tone; focus restored without clipboard use\nFAILURES 0\n");
            }
        }
    }
}
