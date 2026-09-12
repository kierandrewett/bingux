import QtQuick
import QtQuick.Window
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
        blockWrites: true
    }
    Timer {
        id: finish
        interval: 100
        onTriggered: Qt.quit()
    }
    Window {
        id: window
        visible: true
        width: 160
        height: 60
        MarqueeText {
            id: marquee
            width: 80
            text: "A long title that must scroll when visible"
            active: true
        }
        TestCase {
            when: window.visible
            function test_hidden_animation_stops() {
                wait(700);
                verify(Theme.reducedMotion || marquee.offset > 0);
                marquee.visible = false;
                compare(marquee.offset, 0);
                wait(600);
                compare(marquee.offset, 0);
                compare(marquee.revealed, false);
                marquee.visible = true;
                wait(700);
                verify(Theme.reducedMotion || marquee.offset > 0);
                report.setText("FAILURES 0\n");
                finish.start();
            }
        }
    }
}
