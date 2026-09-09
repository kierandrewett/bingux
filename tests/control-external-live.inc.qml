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
    Component { id: sampleComponent; WidgetPreview { width: 280; height: 160 } }
    SignalSpy { id: searchClicks; target: searchPill; signalName: "clicked" }
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
        function test_external_widgets() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                const original = JSON.stringify(BinguxPreferences.data.desktop);
                editor.open(); tryCompare(controlCentre, "visible", true, 3000); tryCompare(controlCentre, "revealScale", 1, 4000); wait(300);
                const header = findChild(controlCentre.body, "controlHeader");
                const centre = DesktopEditing.surfaces.find(surface => surface.zoneName === "control-centre");
                drag(searchPill, topBar, searchPill.width / 2, 16,
                    DesktopEditing.point(header, controlCentre.nativeWindow, 1, 1));
                verify(searchPill.parent === controlCentre.widgetHost);
                compare(editor.desktop.controlLayout.groups["control-centre"][0], "search");
                verify(!editor.layout["top-left"].includes("search"));
                editor.undo(); wait(100); verify(searchPill.parent === leftControls);
                editor.redo(); wait(100); verify(searchPill.parent === controlCentre.widgetHost);
                editor.put("clock", "control-centre", 1); wait(100);
                verify(clockPill.parent === controlCentre.widgetHost); verify(clockPill.y > searchPill.y);
                editor.selectedContainer = "control-centre"; editor.containerDisplay("text");
                verify(searchPill.presentation.showText && !searchPill.presentation.showIcon);
                editor.selectedWidget = "search"; editor.widgetOption("label", "Find something"); editor.widgetOption("display", "both");
                compare(searchPill.presentation.label, "Find something"); verify(searchPill.presentation.showText && searchPill.presentation.showIcon);
                editor.put("label", "control-centre", 2); editor.put("label", "dock", 0); editor.put("label", "control-centre", 3);
                compare(editor.desktop.controlLayout.groups["control-centre"][2], "label:1");
                compare(editor.layout.dock[0], "label:2");
                compare(editor.desktop.controlLayout.groups["control-centre"][3], "label:3");
                editor.selectedWidget = "label:1"; editor.widgetOption("label", "My controls");
                wait(100); const initialLabel = topBar.decorationWidgets.find(item => item.widgetId === "label:1");
                editor.put("icon", "control-centre", 2); wait(150);
                const label = topBar.decorationWidgets.find(item => item.widgetId === "label:1");
                const icon = topBar.decorationWidgets.find(item => item.widgetId === "icon:1");
                verify(label === initialLabel, "Adding another widget keeps existing label instances");
                verify(label.parent === controlCentre.widgetHost && icon.parent === controlCentre.widgetHost);
                compare(label.presentation.label, "My controls");
                verify(centre.entries.some(entry => entry.item === label));
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                drag(label, controlCentre.nativeWindow, label.width / 2, 16, Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                verify(editor.layout.dock.includes("label:1"));
                verify(label.parent === dock.widgetHost, "Moving a label keeps the same instance");
                drag(label, dock, label.width / 2, 16, DesktopEditing.point(header, controlCentre.nativeWindow, 1, 1));
                verify(label.parent === controlCentre.widgetHost);
                for (const id of ["capture", "tray", "privacy", "metrics", "keyboard", "overflow", "controls", "notifications"]) {
                    const item = topBar.defaultControls.find(item => topBar.nameFor(item) === id);
                    editor.put(id, "control-centre", 0); wait(80);
                    verify(item.parent === controlCentre.widgetHost, id + " uses the original control in the real host");
                    verify(topBar.windowFor(item) === controlCentre.nativeWindow);
                }
                verify(Number.isFinite(controlCentre.preferredX) && Number.isFinite(controlCentre.preferredY));
                editor.cancel(); wait(200); compare(JSON.stringify(BinguxPreferences.data.desktop), original);
                editor.open(); editor.put("controls", "control-centre", 0); save();
                controlCentre.visible = true; tryCompare(controlCentre, "revealScale", 1, 3000); wait(150);
                gesture(systemPill, controlCentre.nativeWindow, systemPill.width / 2, 16, ["--click-only"]);
                tryCompare(controlCentre, "visible", false, 2000);
                editor.open(); editor.put("controls", "top-right", 6);
                editor.put("search", "control-centre", 0); editor.put("clock", "control-centre", 1);
                editor.put("label", "control-centre", 2); editor.put("icon", "control-centre", 3);
                editor.selectedWidget = "label:1"; editor.widgetOption("label", "My controls");
                save();
                controlCentre.visible = true; tryCompare(controlCentre, "revealScale", 1, 3000); wait(200);
                gesture(clockPill, controlCentre.nativeWindow, clockPill.width / 2, 16, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "clock"); verify(widgetMenu.anchorWindow === controlCentre.nativeWindow);
                widgetMenu.visible = false; wait(300);
                gesture(clockPill, controlCentre.nativeWindow, clockPill.width / 2, 16, ["--click-only"]);
                tryCompare(calendarPopup, "visible", true, 3000); verify(calendarPopup.anchorWindow === controlCentre.nativeWindow);
                verify(controlCentre.visible, "An anchored child popup retains its parent control centre");
                tryCompare(calendarPopup, "revealScale", 1, 3000);
                verify(calendarPopup.panelY >= clockPill.mapToItem(controlCentre.contentItem, 0, clockPill.height).y);
                capture("external-calendar", clockPill, controlCentre.nativeWindow);
                calendarPopup.visible = false; wait(300);
                capture("external-before-search", searchPill, controlCentre.nativeWindow);
                gesture(searchPill, controlCentre.nativeWindow, searchPill.width / 2, 16, ["--click-only"]);
                compare(searchClicks.count, 1);
                tryCompare(searchOverlay, "visible", true, 4000); verify(!controlCentre.visible);
                capture("external-search", searchPill, controlCentre.nativeWindow);
                searchOverlay.closeSearch();
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
