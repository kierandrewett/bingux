import QtQuick
import QtTest
import Quickshell

ShellRoot {
    DockTooltip {
        id: tip
        text: "Files"
        shown: true
        centreX: 600
        anchorBottom: 160
    }
    TestCase {
        name: "DockTooltipResize"
        when: tip.visible
        function check(condition, message) {
            if (!condition)
                console.error("FAIL: " + message);
            verify(condition, message);
        }
        function test_switch() {
            wait(300);
            const bubble = tip.contentItem.children.find(item => item.visible);
            const column = bubble.children.find(item => item.children?.some(child => child.text === tip.text));
            const label = column.children.find(child => child.text === tip.text);
            const names = ["Files", "Thunderbird Mail", "Helium", "Music", "A very long application name with enough words to wrap onto several lines inside the dock tooltip"];
            for (let n = 0; n < 80; ++n) {
                // Replace some requests before the compositor can configure them.
                if (n % 3 === 0)
                    tip.text = names[4];
                tip.text = names[n % names.length];
                for (let sample = 0; sample < 30; ++sample) {
                    wait(1);
                    if (names.slice(0, 4).includes(label.text))
                        check(label.lineCount === 1, "Short title wrapped during resize: " + label.text);
                    check(column.y + column.height <= bubble.height, "Text overflowed the tooltip bottom during resize");
                    check(label.contentWidth <= label.width + 1, "Text overflowed the tooltip width during resize");
                }
                tryCompare(label, "text", tip.text);
                tryCompare(tip, "width", tip.implicitWidth);
                tryCompare(tip, "height", tip.implicitHeight);
                check(bubble.width <= tip.width && bubble.height <= tip.height, "Settled bubble does not fit the window");
            }
            tip.shown = false;
            tip.text = "Thunderbird Mail";
            wait(30);
            tip.shown = true;
            tryCompare(label, "text", tip.text);
            check(label.lineCount === 1, "Reopened tooltip wrapped a short title");
            console.warn("PASS: 80 dock tooltip switches, pending replacement, long labels and reopen");
        }
    }
}
