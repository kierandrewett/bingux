import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    Timer {
        id: finish
        interval: 200
        onTriggered: Qt.quit()
    }
    FileView {
        id: results
        path: Quickshell.env("BINGUX_METRICS_RESULTS")
    }
    FloatingWindow {
        id: window
        implicitWidth: 800
        implicitHeight: 500
        ProcessTable {
            id: table
            anchors.fill: parent
            records: [
                {
                    pid: 42,
                    name: "example",
                    argv: ["/usr/bin/example", "two words", "", "it's literal", "<b>plain</b>", "x".repeat(500)]
                }
            ]
            applicationFor: () => null
            formatBytes: bytes => String(bytes)
        }
        TestCase {
            name: "ProcessTooltip"
            when: window.visible
            function test_hover() {
                results.setText("FAIL: hover check did not finish\n");
                const row = findChild(table, "processRow_42");
                verify(row !== null);
                const tooltip = findChild(row, "processTooltip");
                mouseMove(row, 60, 15);
                tryCompare(tooltip, "opened", true, 2000);
                verify(tooltip.text.includes("/usr/bin/example 'two words' '' 'it'\"'\"'s literal' '<b>plain</b>' " + "x".repeat(500)));
                verify(tooltip.height > 100, "long argv wraps");
                compare(tooltip.timeout, -1);
                mouseMove(table, 5, 5);
                tryCompare(tooltip, "visible", false);
                table.records = [
                    {
                        pid: 42,
                        name: "example"
                    }
                ];
                table.refresh();
                verify(tooltip.text.includes("Command line unavailable"));
                results.setText("PASS: hover displays full argv, quotes arguments, wraps long values and closes on leave\n");
                finish.start();
            }
        }
    }
}
