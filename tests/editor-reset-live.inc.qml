    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    Process {
        id: nativeInput
        property var gestureArguments: []
        property int captureNumber: 0
        property int resultCode: -1
        onExited: (code, status) => resultCode = code
        stderr: StdioCollector {
            onStreamFinished: if (text)
                console.warn("NATIVE_INPUT", text)
        }
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + (Quickshell.env("BINGUX_RESET_CAPTURE") ? Quickshell.env("BINGUX_RESET_CAPTURE") + "-" + captureNumber + ".png" : ""), "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(nativeInput.gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function click(item, window, hover) {
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y), hover ? "--hover-only" : "--click-only"];
            nativeInput.captureNumber++;
            nativeInput.resultCode = -1;
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 3000);
            compare(nativeInput.resultCode, 0, "Native click succeeds");
        }
        function button(item, text) {
            if (item.text === text && typeof item.clicked === "function")
                return item;
            for (const child of item.children || []) {
                const found = button(child, text);
                if (found)
                    return found;
            }
            return null;
        }
        function readSettings() {
            binguxSettings.read();
            tryCompare(binguxSettings, "busy", false, 4000);
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            wait(350);
        }
        function test_reset() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => BinguxPreferences.data.desktop.layoutVersion === 1, 4000);
                readSettings();
                const editor = desktopCustomiser;
                editor.open();
                wait(350);
                editor.put("clock", "sidebar", 0);
                editor.put("controls-audio", "dock", 0);
                editor.put("control-settings", "palette", 0);
                editor.put("control-network", "sidebar", 0);
                editor.put("label", "top-left", 0);
                editor.selectedWidget = "clock";
                editor.widgetOption("label", "My clock");
                editor.selectContainer("sidebar");
                editor.containerDisplay("both");
                editor.change("dockSize", 40);
                editor.change("dockAlignment", "left");
                editor.change("dockClick", "focus");
                editor.change("dockMiddleClick", "none");
                editor.change("dockScroll", "none");
                editor.change("dockScrollDirection", "reverse");
                editor.change("sidebarEdge", "left");
                editor.change("controlCentre", {
                    vpn: false,
                    dnd: false,
                    nightLight: true,
                    power: true,
                    awake: true
                });
                verify(editor.applications.length > 0);
                editor.put("app:" + editor.appId(editor.applications[0].id), "dock", 0);
                save();
                readSettings();
                const saved = JSON.stringify(binguxSettings.draft.desktop);
                editor.open();
                wait(350);
                const customised = editor.snapshot();
                const pins = JSON.stringify(editor.desktop.dockApps);
                const reset = button(editor.contentItem, "Restore defaults");
                verify(reset !== null);
                verify(reset.enabled, "Reset is enabled after loading defaults");
                click(reset, editor.nativeWindow);
                wait(350);
                compare(JSON.stringify(editor.desktop.widgetOptions), "{}", "Reset clears widget appearance overrides");
                compare(JSON.stringify(editor.desktop.containers), "{}", "Reset clears container appearance overrides");
                compare(JSON.stringify(editor.layout), JSON.stringify(DesktopLayout.defaults()));
                compare(clockPill.parent, centerControls);
                compare(editor.desktop.dockSize, 56);
                compare(editor.desktop.dockAlignment, "center");
                compare(editor.desktop.dockClick, "toggle");
                compare(editor.desktop.dockMiddleClick, "launch");
                compare(editor.desktop.dockScroll, "cycle");
                compare(editor.desktop.dockScrollDirection, "natural");
                compare(editor.desktop.sidebarEdge, "right");
                compare(JSON.stringify(editor.desktop.dockApps), pins, "Reset keeps pinned apps and their order");
                verify(editor.desktop.controlLayout.groups["controls-header"].includes("control-settings"));
                verify(editor.desktop.controlLayout.groups["control-centre"].includes("controls-audio"));
                verify(editor.desktop.controlOrder.includes("network"));
                compare(JSON.stringify(editor.desktop.controlCentre), JSON.stringify(ControlCentreServices.defaultControls));
                editor.undo();
                wait(200);
                compare(editor.snapshot(), customised, "One Undo restores the whole customisation");
                compare(clockPill.parent, terminalSidebar.widgetHost);
                editor.redo();
                wait(200);
                compare(clockPill.parent, centerControls);
                editor.cancel();
                wait(350);
                readSettings();
                compare(JSON.stringify(binguxSettings.draft.desktop), saved, "Cancel leaves the saved customisation untouched");
                compare(clockPill.parent, terminalSidebar.widgetHost);
                terminalSidebar.open();
                wait(350);
                const picker = findChild(terminalSidebar.contentItem, "sidebarContentPicker");
                click(picker, terminalSidebar.widgetWindow);
                tryCompare(terminalSidebar.contentSelector, "revealScale", 1, 3000);
                const customise = button(terminalSidebar.contentSelector.body, "Customise sidebar…");
                verify(customise !== null, "Sidebar menu routes layout changes through the editor");
                verify(customise.width >= customise.implicitWidth, "The menu shows the full customisation label");
                click(customise, terminalSidebar.contentSelector.nativeWindow);
                tryCompare(editor, "visible", true, 4000);
                compare(editor.optionsPage, "Sidebar");
                compare(editor.desktop.sidebarEdge, "left", "Opening customisation does not change the edge");
                editor.optionsPage = "";
                wait(200);
                click(reset, editor.nativeWindow);
                wait(300);
                if (Quickshell.env("BINGUX_RESET_CAPTURE"))
                    click(reset, editor.nativeWindow, true);
                save();
                readSettings();
                compare(JSON.stringify(binguxSettings.draft.desktop.widgetOptions), "{}");
                compare(JSON.stringify(binguxSettings.draft.desktop.dockApps), pins);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
