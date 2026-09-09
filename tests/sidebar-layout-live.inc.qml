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
        function test_sidebar_widgets() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                terminalSidebar.selectContent("notes"); terminalSidebar.open(); wait(350);
                const panel = terminalSidebar.activePanel;
                const oldSize = Qt.size(panel.width, panel.height);
                const oldPoint = panel.mapToItem(terminalSidebar.contentItem, 0, 0);
                compare(oldPoint.x, Theme.gap * 2);
                compare(oldPoint.y, Theme.barHeight + Theme.gap);
                compare(panel.width, terminalSidebar.contentItem.width - Theme.gap * 4);
                compare(panel.height, terminalSidebar.contentItem.height - Theme.barHeight - Theme.gap * 2);
                const editor = desktopCustomiser;
                editor.open(); tryCompare(controlCentre, "revealScale", 1, 4000); wait(300);
                const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "sidebar");
                capture("sidebar-before-drag", clockPill, topBar);
                drag(clockPill, topBar, clockPill.width / 2, 16, Qt.point(area.screenRect.x + 40, area.screenRect.y + Theme.barHeight + 12));
                compare(editor.containerFor("clock"), "sidebar", "Clock placement after native drop: " + JSON.stringify(editor.layout));
                verify(editor.layout.sidebar.indexOf("clock") < editor.layout.sidebar.indexOf("notes"), "Dropping above Notes inserts before its panel");
                compare(clockPill.parent, terminalSidebar.widgetHost);
                compare(terminalSidebar.activePanel, panel);
                editor.undo(); wait(100); compare(clockPill.parent !== terminalSidebar.widgetHost, true);
                editor.redo(); wait(100); compare(clockPill.parent, terminalSidebar.widgetHost);
                editor.put("label", "sidebar", 1); wait(150);
                const label = topBar.decorationWidgets.find(item => item.widgetId === "label:1");
                verify(label); compare(label.parent, terminalSidebar.widgetHost);
                editor.selectedWidget = "label:1"; editor.widgetOption("label", "My sidebar");
                editor.selectContainer("sidebar"); editor.containerDisplay("both");
                compare(label.presentation.label, "My sidebar");
                compare(clockPill.presentation.mode, "both");
                editor.put("label:1", "control-centre", 0); wait(100);
                compare(topBar.decorationWidgets.find(item => item.widgetId === "label:1"), label);
                editor.put("label:1", "sidebar", 1); wait(100);
                compare(topBar.decorationWidgets.find(item => item.widgetId === "label:1"), label);
                for (const id of ["search", "controls", "notifications", "metrics", "keyboard", "tray", "privacy", "capture", "overflow"])
                    editor.put(id, "sidebar", 0);
                wait(200);
                for (const entry of terminalSidebar.externalEntries) compare(entry.item.parent, terminalSidebar.widgetHost);
                verify(panel.height > 0);
                capture("sidebar-all", clockPill, terminalSidebar.editWindow);
                editor.cancel(); wait(350);
                compare(panel.width, oldSize.width); compare(panel.height, oldSize.height);
                compare(panel.mapToItem(terminalSidebar.contentItem, 0, 0), oldPoint);
                editor.open(); wait(300);
                editor.put("clock", "sidebar", 0); editor.put("label", "sidebar", 1); editor.put("icon", "sidebar", 2);
                editor.put("keyboard", "sidebar", 3);
                editor.selectedWidget = "label:1"; editor.widgetOption("label", "My sidebar");
                save();
                gesture(clockPill, terminalSidebar.editWindow, clockPill.width / 2, 16, ["--click-only"]);
                tryCompare(calendarPopup, "visible", true, 3000); tryCompare(calendarPopup, "revealScale", 1, 3000);
                compare(calendarPopup.anchorWindow, terminalSidebar.editWindow);
                capture("sidebar-calendar", clockPill, terminalSidebar.editWindow);
                calendarPopup.visible = false; wait(150);
                terminalSidebar.popOut(); tryCompare(terminalSidebar.detachedSurface, "visible", true, 3000); wait(250);
                compare(clockPill.parent, terminalSidebar.widgetHost);
                compare(topBar.windowFor(clockPill), terminalSidebar.detachedSurface);
                compare(terminalSidebar.activePanel, panel);
                mouseClick(clockPill, clockPill.width / 2, 16);
                tryCompare(calendarPopup, "visible", true, 3000);
                compare(calendarPopup.hostItem, terminalSidebar.detachedSurface.contentItem);
                tryCompare(calendarPopup, "revealScale", 1, 3000);
                verify(calendarPopup.panelX >= 0 && calendarPopup.panelY >= 0);
                verify(calendarPopup.panelX + calendarPopup.body.parent.width <= terminalSidebar.detachedSurface.width);
                capture("sidebar-detached", topBar.contentItem, topBar);
                calendarPopup.visible = false;
                mouseMove(inputSourceSelector, inputSourceSelector.width / 2, 16);
                const tooltip = findChild(inputSourceSelector, "barTooltip");
                verify(tooltip);
                tryCompare(tooltip, "shown", true, 2000);
                compare(tooltip.hostItem, terminalSidebar.detachedSurface.contentItem);
                verify(!tooltip.nativeWindow.visible, "A detached tooltip stays inside the floating window");
                verify(tooltip.followedPosition.x >= 0 && tooltip.followedPosition.x <= terminalSidebar.detachedSurface.width);
                mouseClick(inputSourceSelector, inputSourceSelector.width / 2, 16);
                tryCompare(inputSourceSelector, "menuOpen", true, 3000);
                inputSourceSelector.menuOpen = false;
                const picker = findChild(terminalSidebar.contentItem, "sidebarContentPicker");
                mouseClick(picker, picker.width / 2, picker.height / 2, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.hostItem, terminalSidebar.detachedSurface.contentItem);
                widgetMenu.visible = false;
                terminalSidebar.dockBack(); wait(200);
                compare(calendarPopup.hostItem, null);
                compare(clockPill.parent, terminalSidebar.widgetHost);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
