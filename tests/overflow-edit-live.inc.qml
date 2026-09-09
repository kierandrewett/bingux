    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        property var gestureArguments: []
        property string capturePath: ""
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        property int activations: 0
        function gesture(item, window, options) {
            tryVerify(() => item.width > 0 && item.height > 0, 3000);
            let frame = null;
            verify(item.grabToImage(result => frame = result));
            tryVerify(() => frame !== null, 4000);
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0);
        }
        function test_overflow_widgets() {
            try {
                tray.serviceEnabled = false;
                // Fill the bar on wide displays as well as the compact fixture.
                metricsPill.previewMonitors = metricsPill.monitorNames;
                tray.trayItems = Array.from({length: 18}, (_, index) => ({
                    id: "overflow-" + index, title: "Application " + index, tooltipTitle: "Application " + index,
                    icon: Quickshell.iconPath("applications-other"), menu: null, hasMenu: true, onlyMenu: true,
                }));
                tryVerify(() => topBar.overflows(trayContainer), 3000);
                wait(300);
                const first = findChild(tray, "trayItem-overflow-0");
                const menu = findChild(first, "trayItemMenu");
                menu.actions = [{text: "Open application", enabled: true, isSeparator: false,
                    hasChildren: false, checkState: Qt.Unchecked, icon: "", triggered: () => activations++}];
                gesture(overflowButton, topBar, ["--click-only"]);
                nativeInput.capturePath = "";
                tryCompare(barOverflow, "visible", true, 3000);
                tryCompare(barOverflow, "revealScale", 1, 3000);
                compare(topBar.windowFor(trayContainer), barOverflow.nativeWindow, "Overflow uses the actual host window");
                gesture(first, barOverflow.nativeWindow, ["--click-only"]);
                tryCompare(menu, "visible", true, 3000); tryCompare(menu, "revealScale", 1, 3000);
                compare(menu.anchorWindow, barOverflow.nativeWindow);
                verify(barOverflow.visible, "The parent stays open for its child menu");
                const expected = DesktopEditing.point(first, barOverflow.nativeWindow, first.width, first.height);
                verify(Math.abs(menu.anchorPosition.x - expected.x) < 1);
                verify(Math.abs(menu.anchorPosition.y - expected.y) < 1);
                gesture(findChild(menu.body, "trayMenuEntry0"), menu.nativeWindow, ["--click-only"]);
                compare(activations, 1); tryCompare(menu, "visible", false, 3000);
                barOverflow.visible = false;
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                const original = JSON.stringify(BinguxPreferences.data.desktop);
                const editor = desktopCustomiser;
                editor.open(); tryCompare(binguxSettings, "busy", false, 4000);
                tryCompare(controlCentre, "revealScale", 1, 3000);
                tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                tryVerify(() => topBar.width === topBar.screen.width - terminalSidebar.leftInset - terminalSidebar.rightInset);
                gesture(overflowButton, topBar, ["--click-only"]);
                nativeInput.capturePath = "";
                tryCompare(barOverflow, "visible", true, 3000);
                tryCompare(barOverflow, "revealScale", 1, 3000);
                verify(controlCentre.visible, "Opening More keeps the editor control centre open");
                const outline = findChild(barOverflow.body, "customiseContainerOutline");
                tryVerify(() => {
                    const p = DesktopEditing.point(outline, barOverflow.nativeWindow, 0, 0);
                    return Math.abs(p.x - barOverflow.panelX) < 1 && Math.abs(p.y - barOverflow.panelY) < 1;
                }, 2000, "The editing outline matches the native popup");
                verify(editor.optionsPage === "", "Opening More does not open the inspector");
                const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar");
                tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-overflow.png" : "";
                gesture(first, barOverflow.nativeWindow, ["--hover-only"]);
                nativeInput.capturePath = "";
                gesture(first, barOverflow.nativeWindow, ["--right-click"]);
                compare(editor.selectedWidget, "tray"); compare(editor.optionsPage, "Widget");
                const inspector = findChild(editor.optionsContentItem, "customiseInspector");
                tryVerify(() => inspector.y >= barOverflow.panelY + barOverflow.body.parent.height,
                    2000, "The inspector clears the whole More popup");
                verify(!findChild(first, "barTooltip").shown, "App tooltips are hidden while editing the tray group");
                nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-inspector.png" : "";
                gesture(first, barOverflow.nativeWindow, ["--hover-only"]);
                nativeInput.capturePath = "";
                editor.optionsPage = "";
                gesture(first, barOverflow.nativeWindow, ["--drag-to", String(area.screenRect.x + 40),
                    String(area.screenRect.y + Theme.barHeight + 12)]);
                tryCompare(editor, "draggedId", "", 4000);
                compare(editor.containerFor("tray"), "sidebar");
                compare(findChild(tray, "trayItem-overflow-0"), first, "Drag retains the original tray button");
                editor.undo(); compare(editor.containerFor("tray"), "top-right");
                editor.redo(); compare(editor.containerFor("tray"), "sidebar");
                editor.cancel(); tryCompare(editor, "visible", false, 4000);
                compare(JSON.stringify(BinguxPreferences.data.desktop), original, "Cancel does not save the preview");
                verify(!barOverflow.visible, "Leaving the editor closes its More popup");
                terminalSidebar.open(); tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                tryVerify(() => topBar.width === topBar.screen.width - terminalSidebar.leftInset - terminalSidebar.rightInset);
                tryVerify(() => topBar.overflows(metricsPill), 3000);
                gesture(overflowButton, topBar, ["--click-only"]);
                tryCompare(barOverflow, "visible", true, 3000); tryCompare(barOverflow, "revealScale", 1, 3000);
                gesture(metricsPill, barOverflow.nativeWindow, ["--click-only"]);
                tryCompare(metricsPopup, "visible", true, 3000);
                verify(barOverflow.visible, "Opening a nested performance popup retains More");
                compare(metricsPopup.anchorWindow, barOverflow.nativeWindow);
                metricsPopup.visible = false;
                gesture(metricsPill, barOverflow.nativeWindow, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.widgetId, "metrics"); compare(widgetMenu.anchorWindow, barOverflow.nativeWindow);
                widgetMenu.visible = false;
                editor.open(); tryCompare(binguxSettings, "busy", false, 4000);
                tryCompare(controlCentre, "revealScale", 1, 3000);
                editor.put("overflow", "dock", 0);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                tryCompare(dock.margins, "bottom", 0, 3000);
                compare(overflowButton.parent, dock.widgetHost);
                gesture(overflowButton, dock, ["--click-only"]);
                tryCompare(barOverflow, "visible", true, 3000); tryCompare(barOverflow, "revealScale", 1, 3000);
                verify(barOverflow.anchorAbove); compare(barOverflow.anchorWindow, dock);
                tryVerify(() => Math.abs(barOverflow.panelY + barOverflow.body.parent.height
                    - (dock.popupAnchorTop - Theme.gap)) < 1, 3000, "More opens directly above its dock anchor");
                nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-dock.png" : "";
                gesture(overflowButton, dock, ["--hover-only"]);
                nativeInput.capturePath = "";
                editor.open(); tryCompare(binguxSettings, "busy", false, 4000);
                tryCompare(controlCentre, "revealScale", 1, 3000);
                gesture(overflowButton, dock, ["--click-only"]);
                tryCompare(barOverflow, "visible", true, 3000); tryCompare(barOverflow, "revealScale", 1, 3000);
                gesture(first, barOverflow.nativeWindow, ["--right-click"]);
                tryVerify(() => inspector.y + inspector.height <= barOverflow.panelY,
                    2000, "The inspector clears More when it opens from the dock");
                nativeInput.capturePath = Quickshell.env("BINGUX_GROUP_CAPTURE") ? Quickshell.env("BINGUX_GROUP_CAPTURE") + "-dock-inspector.png" : "";
                gesture(first, barOverflow.nativeWindow, ["--hover-only"]);
                nativeInput.capturePath = "";
                editor.cancel(); tryCompare(editor, "visible", false, 4000);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
