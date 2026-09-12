import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_METRICS_RESULTS")
    }
    Timer {
        id: finish
        interval: 200
        onTriggered: Qt.quit()
    }
    FloatingWindow {
        id: window
        implicitWidth: 400
        implicitHeight: 240
        TooltipBubble {
            id: bubble
            maximumWidth: 200
            text: "Open notifications and settings"
        }
        TestCase {
            when: window.visible
            function cleanupTestCase() {
                finish.start();
            }
            function test_padding() {
                report.setText("FAIL: tooltip sizing checks incomplete\n");
                waitForRendering(bubble);
                const column = bubble.children.find(item => item.children?.some(child => child.text === bubble.text));
                const label = column.children.find(child => child.text === bubble.text);
                verify(label.lineCount > 1);
                verify(bubble.width - label.contentWidth - bubble.horizontalPadding * 2 < 1, "Wrapped tooltip has equal padding instead of unused width on the right");
                bubble.text = "Helium";
                wait(30);
                verify(Math.abs(bubble.width - label.contentWidth - bubble.horizontalPadding * 2) < 1);
                bubble.supportingText = "A supporting description that wraps on another line";
                wait(30);
                const hint = column.children.find(child => child.text === bubble.supportingText);
                verify(bubble.width <= bubble.maximumWidth);
                verify(Math.abs(bubble.width - Math.max(label.contentWidth, hint.contentWidth) - bubble.horizontalPadding * 2) < 1);
                bubble.wrapText = false;
                bubble.supportingText = "";
                bubble.text = "A very long single line that must still stay inside the maximum tooltip width";
                wait(30);
                verify(bubble.width <= bubble.maximumWidth);
                verify(label.truncated);
                report.setText("PASS: balanced tooltip padding, wrapping, supporting text and elision\n");
                finish.start();
            }
        }
    }
}
