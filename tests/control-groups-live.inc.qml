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
        property string capturePath: ""
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    FileView { id: actionReport; path: Quickshell.env("BINGUX_ACTION_REPORT"); watchChanges: true; printErrors: false; onFileChanged: reload() }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, x, y, options) {
            const point = DesktopEditing.point(item, window, x, y);
            nativeInput.gestureArguments = [point.x.toString(), point.y.toString()].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0);
        }
        function drag(item, window, x, y, destination) {
            gesture(item, window, x, y, ["--drag-to", destination.x.toString(), destination.y.toString()]);
            tryCompare(desktopCustomiser, "draggedId", "", 4000); wait(150);
        }
        function capture(name, item, window) {
            const prefix = Quickshell.env("BINGUX_GROUP_CAPTURE");
            if (!prefix) return;
            nativeInput.capturePath = prefix + "-" + name + ".png";
            gesture(item, window, item.width / 2, item.height / 2, ["--hover-only"]);
            nativeInput.capturePath = "";
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
            tryCompare(dock.margins, "bottom", 0, 2000); wait(100);
        }
        function test_native_group_portability() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                editor.open(); tryCompare(controlCentre, "visible", true, 3000); tryCompare(controlCentre, "revealScale", 1, 4000); wait(300);
                const header = findChild(controlCentre.body, "controlHeader");
                const audio = findChild(controlCentre.body, "controlAudioRows");
                const quick = findChild(controlCentre.body, "controlQuickRows");
                const settings = findChild(header, "controlSettings");
                const account = findChild(header, "controlUserAccount");
                const output = findChild(audio, "controlOutputRow");
                const input = findChild(audio, "controlInputRow");
                const network = findChild(quick, "controlNetwork");
                const bluetooth = findChild(quick, "controlBluetooth");
                const nativeParent = header.parent;
                output.node = testOutput; input.node = testInput;
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                const originalChildren = JSON.stringify(editor.desktop.controlLayout.groups["controls-header"]);
                const centreSurface = DesktopEditing.surfaces.find(surface => surface.zoneName === "control-centre");
                const headerGrip = findChild(centreSurface, "customise-group-handle-controls-header");
                verify(headerGrip !== null, "The group has a visible drag affordance");
                gesture(headerGrip, controlCentre.nativeWindow, headerGrip.width / 2, headerGrip.height / 2, ["--click-only"]);
                compare(editor.selectedContainer, "controls-header"); compare(editor.optionsPage, "Container");
                compare(editor.inspectionAnchor.width, header.width);
                drag(headerGrip, controlCentre.nativeWindow, headerGrip.width / 2, headerGrip.height / 2,
                    Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                compare(header.parent, dock.widgetHost);
                compare(settings.parent, header); verify(settings.barLayout && !settings.placed);
                compare(header.height, Theme.barHeight);
                compare(JSON.stringify(editor.desktop.controlLayout.groups["controls-header"]), originalChildren);
                verify(!editor.desktop.controlLayout.groups["control-centre"].includes("controls-header"));
                editor.undo(); wait(150); compare(header.parent, nativeParent); verify(!settings.barLayout);
                editor.redo(); wait(150); compare(header.parent, dock.widgetHost);
                // A child can leave and return to the same moved group.
                const top = DesktopEditing.surfaces.find(surface => surface.zoneName === "top-left");
                drag(settings, dock, 16, 16, Qt.point(top.screenRect.x + 10, top.screenRect.y + 16));
                compare(settings.parent, leftControls);
                verify(!editor.desktop.controlLayout.groups["controls-header"].includes("control-settings"));
                drag(settings, topBar, 16, 16, DesktopEditing.point(account, dock, 1, 16));
                compare(settings.parent, header); compare(header.parent, dock.widgetHost);
                compare(editor.desktop.controlLayout.groups["controls-header"][0], "control-settings");
                editor.selectedContainer = "dock"; editor.containerDisplay("text");
                verify(settings.presentation.showText && !settings.presentation.showIcon);
                editor.selectedContainer = "controls-header"; editor.containerDisplay("icons");
                verify(settings.presentation.showIcon && !settings.presentation.showText);
                editor.containerDisplay("inherit"); verify(settings.presentation.showText && !settings.presentation.showIcon);
                editor.containerDisplay("icons");
                editor.selectedWidget = "control-settings"; editor.widgetOption("display", "both"); editor.widgetOption("label", "Preferences");
                verify(settings.presentation.showIcon && settings.presentation.showText);
                compare(settings.displayedLabel, "Preferences");
                editor.widgetOption("display", "inherit"); editor.widgetOption("label", "");
                editor.selectedContainer = "dock"; editor.containerDisplay("native");
                editor.selectedContainer = "controls-header"; editor.containerDisplay("native");
                save();
                gesture(settings, dock, 16, 16, ["--hover-only"]);
                const tooltip = findChild(settings, "iconButtonBarTooltip");
                tryCompare(tooltip, "shown", true, 2000);
                const tooltipPoint = DesktopEditing.point(settings, dock, settings.width / 2, settings.height);
                compare(tooltip.centreX, tooltipPoint.x); compare(tooltip.bottomY, tooltipPoint.y);
                const tooltipSurface = tooltip.nativeWindow;
                compare(tooltipSurface.margins.top + tooltipSurface.height + Theme.gap, dock.popupAnchorTop, "The tooltip clears the dock surface");
                capture("header", settings, dock);
                gesture(header, dock, account.x + account.width + 4, 16, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "controls-header");
                widgetMenu.visible = false; wait(300);
                gesture(settings, dock, 16, 16, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "control-settings"); compare(widgetMenu.anchorWindow, dock);
                widgetMenu.visible = false; wait(300);
                gesture(settings, dock, 16, 16, ["--click-only"]);
                tryVerify(() => actionReport.text().includes("gnome-control-center"), 3000);
                editor.open(); editor.put("controls-header", "control-centre", 0); wait(200);
                compare(header.parent, nativeParent); compare(settings.parent, header); verify(!settings.barLayout);
                drag(audio, controlCentre.nativeWindow, audio.width - 4, output.height + 6,
                    Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                compare(audio.parent, dock.widgetHost); compare(output.parent, audio); compare(input.parent, audio);
                compare(audio.height, Theme.barHeight); verify(output.x < input.x);
                save();
                capture("audio", audio, dock);
                const mute = findChild(output, "controlMute");
                gesture(mute, dock, mute.width / 2, mute.height / 2, ["--click-only"]); verify(outputAudio.muted);
                const nav = findChild(input, "controlInputDetails");
                gesture(nav, dock, nav.width / 2, nav.height / 2, ["--click-only"]);
                tryCompare(controlCentre, "visible", true, 3000); compare(controlCentre.anchorWindow, dock); verify(controlCentre.anchorAbove);
                compare(controlCentre.deviceControls.audioTab, "input");
                controlCentre.visible = false; wait(300);
                editor.open(); editor.put("controls-audio", "control-centre", 1); wait(200);
                compare(audio.parent, nativeParent); verify(input.y > output.y);
                editor.put("controls-tiles", "top-left", 0); wait(200);
                compare(quick.parent, leftControls); compare(network.parent, quick); compare(bluetooth.parent, quick);
                const quickOrder = JSON.stringify(editor.desktop.controlOrder);
                editor.change("controlOrder", []); wait(100);
                verify(quick.width >= Theme.barHeight && quick.height >= Theme.barHeight, "Empty groups retain an editor drop target");
                editor.undo(); wait(100); compare(JSON.stringify(editor.desktop.controlOrder), quickOrder);
                compare(quick.height, Theme.barHeight); verify(bluetooth.x > network.x);
                save();
                capture("quick", network, topBar);
                gesture(network, topBar, network.width / 2, network.height / 2, ["--click-only"]);
                tryCompare(controlCentre, "visible", true, 3000); compare(controlCentre.detailPage, "network"); verify(!controlCentre.anchorAbove);
                controlCentre.visible = false; wait(300);
                editor.open(); editor.put("controls-tiles", "control-centre", 3); save();
                compare(quick.parent, nativeParent); verify(!network.barLayout);
                // Leave an imported, saved arrangement for a fresh-process check.
                editor.open();
                editor.put("controls-header", "top-left", 0);
                editor.put("controls-audio", "dock", 0);
                editor.put("controls-tiles", "top-center", 0);
                editor.selectedContainer = "controls-header"; editor.containerDisplay("text");
                editor.selectedWidget = "control-settings"; editor.widgetOption("display", "both"); editor.widgetOption("label", "Preferences");
                save();
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
