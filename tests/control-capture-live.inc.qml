    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    FileView { id: captureActions; path: Quickshell.env("BINGUX_CAPTURE_ACTION_REPORT"); watchChanges: true; printErrors: false; onFileChanged: reload() }
    Process {
        id: nativeInput
        property int resultCode: -1
        property var gestureArguments: []
        onExited: code => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised && captureTool.ready
        function commands(name) {
            return captureActions.text().trim().split("\n").filter(line => line && JSON.parse(line).command === name).length;
        }
        function gesture(item, window, options) {
            verify(waitForPolish(window.contentItem.Window.window, 2000));
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options || ["--click-only"]);
            nativeInput.resultCode = -1; nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000); compare(nativeInput.resultCode, 0);
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
            tryCompare(dock.margins, "bottom", 0, 2000); wait(150);
        }
        function start(delay) {
            verify(captureTool.configureOptions(JSON.stringify({kind: "recording", target: "screen", delay: delay})).ok);
            captureTool.take();
            tryCompare(captureTool, "state", delay ? "countdown" : "recording", 3000);
            if (delay) tryCompare(captureTool, "countdown", delay, 3000);
            tryCompare(captureStatus, "visible", true, 3000);
        }
        function test_moved_capture() {
            let phase = "initial import";
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                const original = captureStatus;
                const originalParent = captureStatus.parent;
                start(0);
                phase = "drag active recording";
                editor.open(); tryCompare(controlCentre, "revealScale", 1, 3000); wait(250);
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                if (topBar.overflows(captureStatus)) {
                    gesture(overflowButton, topBar.windowFor(overflowButton));
                    tryCompare(barOverflow, "visible", true, 3000);
                    tryCompare(barOverflow, "revealScale", 1, 3000);
                }
                gesture(captureStatus, topBar.windowFor(captureStatus), ["--hover-only"]);
                gesture(captureStatus, topBar.windowFor(captureStatus), ["--drag-to", String(dockArea.screenRect.x + 12), String(dockArea.screenRect.y + 12)]);
                tryCompare(editor, "draggedId", "", 3000);
                tryCompare(captureStatus, "parent", dock.widgetHost);
                compare(captureTool.state, "recording"); compare(commands("stop"), 0);
                editor.undo(); tryCompare(captureStatus, "parent", originalParent);
                editor.redo(); tryCompare(captureStatus, "parent", dock.widgetHost);
                editor.selectedWidget = "capture";
                editor.widgetOption("display", "both"); editor.widgetOption("label", "Capture status");
                for (const zone of ["dock", "top-left", "control-centre", "sidebar"]) {
                    phase = zone + " placement";
                    if (!editor.visible) editor.open();
                    editor.put("capture", zone, 0); save();
                    compare(captureStatus, original);
                    compare(captureStatus.parent, topBar.hostFor(captureStatus));
                    compare(captureStatus.presentation.label, "Capture status");
                    verify(captureStatus.presentation.showText && captureStatus.presentation.showIcon);
                    if (zone === "control-centre") {
                        controlCentre.visible = true; tryCompare(controlCentre, "revealScale", 1, 3000);
                    } else if (zone === "sidebar") {
                        terminalSidebar.open(); tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                    }
                    if (!captureTool.recording) start(0);
                    if (zone === "control-centre") {
                        controlCentre.visible = false; tryCompare(controlCentre, "retained", false, 3000);
                        verify(captureStatus.active && !captureStatus.visible);
                        verify(topBar.availableControls.includes(captureStatus), "A hidden host does not change capture availability");
                        controlCentre.visible = true; tryCompare(controlCentre, "revealScale", 1, 3000);
                    }
                    const host = topBar.windowFor(captureStatus);
                    phase = zone + " recording stop";
                    const stops = commands("stop");
                    verify(captureStatus.interactive && captureStatus.isRecording);
                    compare(captureStatus.tooltip, "Stop screen recording");
                    verify(captureTool.elapsed >= 125);
                    gesture(captureStatus, host);
                    tryCompare(captureTool, "state", "finalizing", 3000);
                    tryVerify(() => commands("stop") === stops + 1, 2000);
                    phase = zone + " saving ignores input";
                    verify(!captureStatus.interactive); compare(captureStatus.label, "Saving…");
                    gesture(captureStatus, host);
                    compare(commands("stop"), stops + 1);
                    tryCompare(captureTool, "state", "saved", 5000);
                    tryCompare(captureStatus, "visible", false, 2000);
                    phase = zone + " countdown cancel";
                    start(10); compare(captureStatus.label, "10");
                    compare(captureStatus.tooltip, "Cancel capture");
                    gesture(captureStatus, host, ["--shift-right-click"]);
                    tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "capture");
                    widgetMenu.visible = false; tryCompare(widgetMenu, "retained", false, 3000);
                    compare(captureTool.state, "countdown");
                    gesture(captureStatus, host);
                    tryCompare(captureTool, "state", "idle", 3000);
                    tryVerify(() => commands("stop") === stops + 2, 2000);
                    tryCompare(captureStatus, "visible", false, 2000);
                    if (controlCentre.visible) { controlCentre.visible = false; tryCompare(controlCentre, "retained", false, 3000); }
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", phase, error.message, error.stack);
                layoutReport.setText("FAIL " + phase + " " + error.stack);
            }
        }
    }
