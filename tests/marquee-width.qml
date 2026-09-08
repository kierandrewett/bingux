import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    FloatingWindow {
        id: window
        implicitWidth: 416; implicitHeight: 240
        color: Theme.shellSurface
        ColumnLayout {
            id: rows
            anchors.fill: parent
            anchors.margins: 16
            Repeater {
                model: 3
                ControlRow {
                    required property int index
                    title: "Studio speakers with a descriptive device name"
                    iconName: "audio-speakers-symbolic"
                    selected: index === 0
                }
            }
        }
        TestCase {
            id: test
            property bool saved: false
            when: window.visible
            function test_widths() {
                report.setText("FAILURES 1\n");
                waitForRendering(rows);
                let widths = [];
                for (const row of rows.children) {
                    const title = findChild(row, "controlRowTitle");
                    if (!title) continue;
                    widths.push(title.width);
                }
                report.setText(JSON.stringify(widths) + "\nFAILURES 1\n");
                compare(widths.length, 3);
                verify(widths[1] >= widths[0], "Unselected rows use at least as much text width as the selected row");
                verify(widths[2] >= widths[0], "Later rows retain the full text width");
                const lastRow = rows.children[2];
                lastRow.selected = true;
                rows.children[0].selected = false;
                wait(50);
                const firstTitle = findChild(rows.children[0], "controlRowTitle");
                const lastTitle = findChild(lastRow, "controlRowTitle");
                verify(firstTitle.width >= lastTitle.width, "Changing selection never collapses the old row");
                mouseMove(lastTitle, lastTitle.width / 2, lastTitle.height / 2);
                wait(750);
                verify(Theme.reducedMotion ? lastTitle.offset === 0 : lastTitle.offset > 0);
                mouseMove(window.contentItem, 1, 1);
                wait(50);
                compare(lastTitle.offset, 0);
                rows.grabToImage(result => { test.saved = result.saveToFile("/tmp/bingux-marquee-width.png"); });
                tryCompare(test, "saved", true, 2000);
                report.setText(JSON.stringify(widths) + "\nFAILURES 0\n");
            }
        }
    }
}
