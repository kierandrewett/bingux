import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    QtObject {
        id: metrics
        property bool desktopStateAvailable: true
        property var inputSources: [
            {type: "xkb", id: "gb", shortName: "en₁", displayName: "English (UK)"},
            {type: "xkb", id: "us", shortName: "en₂", displayName: "English (US)"},
            {type: "xkb", id: "de", shortName: "de", displayName: "German"}
        ]
        property var currentInputSource: inputSources[0] || null
        property string inputSourceLabel: currentInputSource ? currentInputSource.shortName : ""
    }
    FileView { id: result; path: Quickshell.env("BINGUX_KEYBOARD_TEST_RESULTS") }
    Process {
        id: selectionReader
        command: ["cat", Quickshell.env("BINGUX_KEYBOARD_TEST_SELECTION")]
        stdout: StdioCollector { id: selectionOutput }
    }
    Timer { id: finish; interval: 100; onTriggered: Qt.quit() }
    FloatingWindow {
        id: top
        implicitWidth: 500; implicitHeight: 32
        InputSourceSelector {
            id: selector
            parentWindow: top
            metrics: metrics
            shortcutsEnabled: false
            gnoblinCtlPath: Quickshell.env("BINGUX_KEYBOARD_TEST_CLI")
        }
        TestCase {
            when: top.visible
            function test_switching() {
                result.setText("FAIL: keyboard checks did not finish\n");
                waitForRendering(selector);
                compare(selector.displayLabel, "en₁");
                selector.openMenu(); wait(200);
                const popup = findChild(selector, "keyboardLayoutPopup");
                verify(popup.keyboardInteractive, "manual menu accepts keyboard input");
                verify(selector.menuOpen); compare(selector.selectedIndex, 0);
                selector.cycleSource(false); compare(selector.selectedIndex, 1);
                selector.cycleSource(false); compare(selector.selectedIndex, 2);
                selector.cycleSource(false); compare(selector.selectedIndex, 0);
                selector.cycleSource(true); compare(selector.selectedIndex, 2);
                tryCompare(selector, "canSelect", true, 2000);
                verify(selector.menuOpen, "cycling keeps the dropdown visible");
                verify(!popup.keyboardInteractive, "shortcut preview never requests keyboard focus");
                selector.finishCycle();
                verify(!selector.menuOpen, "Super release closes the preview immediately");
                const label = findChild(selector, "keyboardLayoutLabel");
                const stableWidth = selector.width;
                metrics.currentInputSource = metrics.inputSources[1];
                compare(selector.displayLabel, "en₂", "same-language layouts have distinct top-bar labels");
                wait(250);
                result.setText("FAIL: animated label value=" + label.value + " displayed=" + label.displayedValue + " width=" + selector.width + " expected=" + stableWidth + "\n");
                tryCompare(label, "displayedValue", 1, 1000);
                compare(selector.width, stableWidth, "layout label animation keeps a stable hit target");
                selector.openMenu(); wait(200); compare(selector.selectedIndex, 1);
                selector.selectCurrentSource();
                tryCompare(selector, "menuOpen", false, 2000);
                selectionReader.running = true;
                tryCompare(selectionReader, "running", false, 2000);
                compare(selectionOutput.text.trim(), "set-input-source xkb us", "manual selection dispatches the selected source");
                selector.openMenu(); wait(200);
                metrics.inputSources = [metrics.inputSources[1]];
                compare(selector.selectedIndex, 0, "selection survives a reordered source list");
                selector.cycleSource(false); compare(selector.selectedIndex, 0, "one layout remains selectable");
                tryCompare(selector, "canSelect", true, 2000);
                metrics.inputSources = [];
                verify(!selector.menuOpen, "empty sources close the menu");
                result.setText("PASS: forward/backward wrap, rapid cycles, distinct labels, manual selection, reordered/empty sources\n");
                finish.start();
            }
        }
    }
}
