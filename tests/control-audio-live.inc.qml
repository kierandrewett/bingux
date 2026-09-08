    QtObject { id: outputAudio; property bool muted: false; property real volume: 0.65 }
    QtObject { id: inputAudio; property bool muted: false; property real volume: 0.4 }
    QtObject { id: testOutput; property bool ready: true; property var audio: outputAudio }
    QtObject { id: testInput; property bool ready: true; property var audio: inputAudio }
    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        property var gestureArguments: []
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    TestCase {
        id: controlAudioTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, x, y, options) {
            const point = DesktopEditing.point(item, window, x, y);
            nativeInput.gestureArguments = [point.x.toString(), point.y.toString()].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0, "The compositor accepts the native gesture");
        }
        function drag(item, x, y, destination) {
            gesture(item, controlCentre.nativeWindow, x, y, ["--drag-to", destination.x.toString(), destination.y.toString()]);
            tryCompare(desktopCustomiser, "draggedId", "", 4000);
            wait(150);
        }
        function test_native_audio() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                editor.open();
                tryCompare(controlCentre, "visible", true, 3000);
                tryCompare(controlCentre, "revealScale", 1, 4000);
                wait(300);
                const output = findChild(controlCentre.body, "controlOutputRow");
                const microphone = findChild(controlCentre.body, "controlInputRow");
                const audioRows = output.parent;
                output.node = testOutput; microphone.node = testInput;
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                drag(output, 20, 16, Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                compare(output.parent, dock.widgetHost);
                compare(output.node, testOutput);
                verify(!editor.desktop.controlLayout.groups["controls-audio"].includes("control-volume"));
                editor.undo(); wait(150); compare(output.parent, audioRows);
                editor.redo(); wait(150); compare(output.parent, dock.widgetHost);
                editor.selectedContainer = "dock"; editor.containerDisplay("text");
                editor.selectedWidget = "control-volume"; editor.widgetOption("label", "Speakers");
                const muteFace = findChild(output, "controlMute");
                tryCompare(muteFace, "displayedLabel", "Speakers");
                verify(muteFace.presentation.showText && !muteFace.presentation.showIcon);
                editor.widgetOption("display", "both"); editor.widgetOption("icon", "starred-symbolic");
                verify(muteFace.presentation.showIcon && muteFace.presentation.icon === "starred-symbolic");
                editor.widgetOption("display", "inherit"); editor.widgetOption("label", ""); editor.widgetOption("icon", "");
                editor.containerDisplay("native");
                editor.put("control-microphone", "top-left", 0);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                compare(output.parent, dock.widgetHost); compare(microphone.parent, leftControls);
                compare(BinguxPreferences.data.desktop.layout.dock[0], "control-volume");
                verify(!BinguxPreferences.data.desktop.controlLayout.groups["controls-audio"].includes("control-volume"));
                wait(400);
                for (const entry of [
                    {item: microphone, window: topBar, mute: "controlMicrophoneQuick", slider: "controlMicrophoneVolume", nav: "controlInputDetails", audio: inputAudio, tab: "input"},
                    {item: output, window: dock, mute: "controlMute", slider: "controlVolume", nav: "controlSoundDetails", audio: outputAudio, tab: "output"}
                ]) {
                    const mute = findChild(entry.item, entry.mute);
                    gesture(mute, entry.window, mute.width / 2, mute.height / 2, ["--hover-only"]);
                    wait(Theme.tooltipDelay + 100);
                    const tooltip = findChild(mute, "iconButtonBarTooltip");
                    tryCompare(tooltip, "shown", true, 1000);
                    compare(tooltip.bottomY, DesktopEditing.point(mute, entry.window, mute.width / 2, mute.height).y, "Tooltip follows the actual layer window");
                    gesture(mute, entry.window, mute.width / 2, mute.height / 2, ["--click-only"]);
                    verify(entry.audio.muted, "Moved audio mute retains its node action");
                    const slider = findChild(entry.item, entry.slider);
                    gesture(slider, entry.window, slider.width * 0.8, slider.height / 2, ["--click-only"]);
                    verify(!entry.audio.muted && entry.audio.volume > 0.7, "Moved slider changes volume and unmutes");
                    const beforeWheel = entry.audio.volume;
                    mouseWheel(slider, slider.width / 2, slider.height / 2, 0, -120);
                    tryVerify(() => entry.audio.volume < beforeWheel, 1000, "Moved slider keeps wheel adjustment");
                    const navigation = findChild(entry.item, entry.nav);
                    gesture(navigation, entry.window, navigation.width / 2, navigation.height / 2, ["--click-only"]);
                    tryCompare(controlCentre, "visible", true, 3000);
                    compare(controlCentre.movedAnchor, entry.item, "Device popup anchors to the moved audio widget");
                    compare(controlCentre.detailPage, "audio");
                    compare(controlCentre.deviceControls.audioTab, entry.tab);
                    wait(300);
                    gesture(navigation, entry.window, navigation.width / 2, navigation.height / 2, ["--hover-only"]);
                    controlCentre.visible = false; wait(400);
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
