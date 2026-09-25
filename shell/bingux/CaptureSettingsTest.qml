import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    UiSession {
        sessionName: "capture-settings-test"
        state: ({
                visible: capture.opened,
                surface: "bingux-capture",
                companions: ["bingux-capture-controls"],
                companionsAbove: true
            })
    }
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CAPTURE_TEST_RESULTS")
    }
    CaptureTool {
        id: capture
        screen: Quickshell.screens[0]
    }
    Timer {
        interval: 300
        running: true
        onTriggered: {
            capture.configureOptions(JSON.stringify({
                kind: "recording",
                audio: "none"
            }));
            capture.open();
            capture.optionsOpen = true;
        }
    }
    Timer {
        interval: 10000
        running: true
        onTriggered: Qt.quit()
    }
    TestCase {
        name: "CaptureSettings"
        parent: capture.previewItem
        when: capture.opened && capture.ready && capture.previewItem !== null && capture.previewItem.width > 0
        function find(item, name) {
            if (item.objectName === name)
                return item;
            for (const child of item.children || []) {
                const found = find(child, name);
                if (found)
                    return found;
            }
            return null;
        }
        function test_settings() {
            report.setText("FAIL: capture settings interactions\n");
            const surface = capture.previewItem;
            waitForPolish(surface);
            const toolbar = find(surface, "captureToolbar");
            tryCompare(toolbar, "settingsProgress", 1, 1000);
            tryCompare(toolbar, "resizeProgress", 1, 1000);
            const back = find(surface, "captureSettingsBack");
            compare(back.width, 36);
            compare(back.height, 36);
            const settingsPage = find(surface, "captureSettingsPage");
            const settledY = settingsPage.mapToItem(surface, 0, 0).y;
            capture.optionsOpen = false;
            tryCompare(toolbar, "settingsProgress", 0, 1000);
            wait(100);
            capture.optionsOpen = true;
            let maximumTravel = 0;
            for (let frame = 0; frame < 14; frame++) {
                wait(16);
                maximumTravel = Math.max(maximumTravel, Math.abs(settingsPage.mapToItem(surface, 0, 0).y - settledY));
                const frameDirectory = Quickshell.env("BINGUX_CAPTURE_TEST_FRAMES");
                if (frameDirectory && [0, 3, 7, 13].includes(frame)) {
                    const framePath = frameDirectory + "/capture-transition-" + frame + ".png";
                    toolbar.grabToImage(result => result.saveToFile(framePath));
                }
            }
            report.setText("FAIL: settings content travels " + maximumTravel + "px during resize\n");
            verify(maximumTravel <= 1, "Reveal settings in place, not travelling with the resizing top edge: " + maximumTravel + "px");
            capture.optionsOpen = false;
            for (let frame = 0; frame < 14; frame++) {
                wait(16);
                compare(settingsPage.mapToItem(surface, 0, 0).y, settledY, "Closing keeps settings rows stationary too");
            }
            // Reversing direction part-way through must remain continuous.
            capture.optionsOpen = true;
            wait(50);
            const interruptedHeight = toolbar.height;
            capture.optionsOpen = false;
            compare(toolbar.height, interruptedHeight);
            tryCompare(toolbar, "resizeProgress", 0, 1000);
            capture.optionsOpen = true;
            tryCompare(toolbar, "resizeProgress", 1, 1000);
            tryCompare(toolbar, "settingsProgress", 1, 1000);
            const systemSwitch = find(surface, "captureSystemAudioSwitch");
            const microphoneSwitch = find(surface, "captureMicrophoneAudioSwitch");
            verify(systemSwitch !== null && microphoneSwitch !== null);
            verify(systemSwitch.enabled && microphoneSwitch.enabled);
            mouseClick(systemSwitch);
            compare(capture.optionsSnapshot().audio, "system");
            mouseClick(microphoneSwitch);
            compare(capture.optionsSnapshot().audio, "both");
            mouseClick(systemSwitch);
            compare(capture.optionsSnapshot().audio, "microphone");
            microphoneSwitch.forceActiveFocus();
            keyClick(Qt.Key_Space);
            compare(capture.optionsSnapshot().audio, "none");
            const encoding = find(surface, "captureChoiceEncoding");
            verify(!encoding.visible);
            mouseClick(find(surface, "captureAdvancedSettings"));
            verify(encoding.visible);
            wait(250);
            mouseClick(find(surface, "captureAdvancedSettings"));
            verify(!encoding.visible);
            wait(250);
            const high = find(surface, "captureChoiceQualityHigh");
            mouseClick(high);
            compare(capture.optionsSnapshot().quality, "high");
            const compact = find(surface, "captureChoiceQualityCompact");
            compact.forceActiveFocus();
            keyClick(Qt.Key_Space);
            compare(capture.optionsSnapshot().quality, "compact");
            const page = find(surface, "captureSettingsPage");
            const directory = capture.optionsSnapshot().directory;
            mouseClick(find(surface, "captureSaveFolder"));
            tryCompare(page, "pickingFolder", true, 2000);
            compare(capture.pickingFolder, true);
            page.folderDialog.reject();
            tryCompare(capture, "pickingFolder", false, 2000);
            compare(capture.optionsSnapshot().directory, directory);
            wait(100);
            mouseClick(find(surface, "captureSettingsBack"));
            compare(capture.optionsOpen, false);
            tryCompare(toolbar, "settingsProgress", 0, 1000);
            tryCompare(toolbar, "resizeProgress", 0, 1000);
            compare(toolbar.width, 320);
            compare(toolbar.height, 176);
            mouseClick(find(surface, "captureSettingsToggle"));
            tryCompare(toolbar, "settingsProgress", 1, 1000);
            tryCompare(toolbar, "resizeProgress", 1, 1000);
            compare(toolbar.width, 360);
            compare(back.width, back.height);
            compact.forceActiveFocus();
            keyClick(Qt.Key_Escape);
            compare(capture.optionsOpen, false, "Escape returns from focused settings control");
            compare(capture.opened, true, "First Escape keeps capture open");
            tryCompare(toolbar, "settingsProgress", 0, 1000);
            keyClick(Qt.Key_Escape);
            tryCompare(capture, "opened", false, 1000);
            report.setText("PASS: stationary pages during resize, interruption without snapping, independent audio switches, keyboard toggle, Advanced settings, inline choices, square Back, Escape to controls then Escape to close\n");
        }
    }
}
