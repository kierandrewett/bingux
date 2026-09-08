import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: result; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 300
        implicitHeight: 100
        RollingNumber { id: number; text: "129%"; value: 129 }
        TextMetrics {
            id: reference
            text: "1"
            font.family: Theme.fontFamily
            font.pixelSize: number.pixelSize
            font.weight: number.fontWeight
            font.features: ({"tnum": 1})
        }
        TestCase {
            when: window.visible
            function test_independent_digits() {
                wait(20);
                const first = findChild(number, "rollingGlyph0");
                compare(first.width, reference.advanceWidth, "Use font advance, including character spacing");
                number.text = "139%";
                number.value = 139;
                wait(40);
                verify(number.animating);
                verify(!first.rolling);
                verify(findChild(number, "rollingGlyph1").rolling);
                verify(!findChild(number, "rollingGlyph2").rolling);
                verify(!findChild(number, "rollingGlyph3").rolling);
                const glyph = findChild(first, "oldRollingDigit");
                compare(glyph.y, (first.height - glyph.height) / 2, "Unchanged digit stays still");
                tryCompare(number, "displayedText", "139%", 500);
            }
            function cleanupTestCase() { result.setText("FAILURES " + qtest_results.failCount + "\n"); Qt.quit(); }
        }
    }
}
