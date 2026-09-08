    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        property var gestureArguments: []
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    Component { id: sampleComponent; WidgetPreview { width: 280; height: 160 } }
    TestCase {
        id: controlLayoutTest
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
        function test_native_groups() {
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
                const original = JSON.stringify(editor.desktop.controlLayout);
                const header = findChild(controlCentre.body, "controlHeader");
                const settings = findChild(controlCentre.body, "controlSettings");
                const account = findChild(controlCentre.body, "controlUserAccount");
                const output = findChild(controlCentre.body, "controlOutputRow");
                const microphone = findChild(controlCentre.body, "controlInputRow");
                drag(settings, 16, 16, DesktopEditing.point(account, controlCentre.nativeWindow, 1, 16));
                compare(editor.desktop.controlLayout.groups["controls-header"][0], "control-settings");
                verify(settings.x < account.x, "The native settings button follows the drop");
                const audioRows = findChild(controlCentre.body, "controlAudioRows");
                drag(header, settings.x + settings.width + 4, 16,
                    DesktopEditing.point(audioRows, controlCentre.nativeWindow, audioRows.width - 2, audioRows.height - 2));
                compare(editor.desktop.controlLayout.groups["control-centre"][0], "controls-audio", "A native group can move as a unit");
                verify(audioRows.y < header.y);
                editor.undo(); wait(200);
                drag(output, output.width / 2, output.height / 2,
                    DesktopEditing.point(microphone, controlCentre.nativeWindow, microphone.width - 2, microphone.height - 2));
                compare(editor.desktop.controlLayout.groups["controls-audio"][0], "control-microphone");
                verify(microphone.y < output.y, "The native sliders reorder");
                const palette = editor.preview.paletteRect;
                drag(output, 20, 16, Qt.point(palette.x + 30, palette.y + 25));
                verify(!output.visible, "Dragging to the palette removes the native audio row");
                editor.undo(); wait(150); verify(output.visible);
                editor.redo(); wait(150); verify(!output.visible);
                editor.undo();
                compare(findChild(controlCentre.body, "controlOutputRow"), output, "Undo keeps the same control");
                for (const spec of ControlLayout.widgets) {
                    const sample = sampleComponent.createObject(editor.contentItem, {widgetId: spec.id});
                    tryVerify(() => sample.previewControl !== null, 1500, "Native preview loads: " + spec.id);
                    verify(sample.visualItem.width > 0 && sample.visualItem.height > 0, "Preview has geometry: " + spec.id);
                    sample.destroy();
                }
                editor.put("controls-audio", "palette", 0); wait(150);
                verify(!audioRows.visible);
                editor.put("control-volume", "control-centre", 0); wait(150);
                verify(audioRows.visible && output.visible, "Adding an audio control restores its missing group");
                editor.cancel(); wait(300);
                compare(JSON.stringify(BinguxPreferences.data.desktop.controlLayout), original, "Cancel preserves saved groups");
                editor.open();
                editor.change("controlLayout", JSON.parse(original));
                editor.put("control-volume", "control-centre", 1);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                compare(BinguxPreferences.data.desktop.controlLayout.groups["controls-audio"][0], "control-microphone");
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
