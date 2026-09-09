    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: compactInput
        property var gestureArguments: []
        property int resultCode: -1
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(gestureArguments)
        onExited: (code, status) => resultCode = code
    }
    TestCase {
        id: compactTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, options) {
            // An unchanged item may never request the next frame that
            // waitForRendering waits for. Request a frame before mapping input.
            let frame = null;
            verify(item.grabToImage(result => frame = result));
            tryVerify(() => frame !== null, 4000);
            const point = DesktopEditing.point(item, window, item.width / 2, Math.min(50, item.height / 2));
            compactInput.gestureArguments = [String(point.x), String(point.y)].concat(options);
            compactInput.running = true;
            tryCompare(compactInput, "running", false, 4000);
            compare(compactInput.resultCode, 0);
        }
        function test_palette_and_containers() {
            try {
                if (Quickshell.env("BINGUX_TEST_SCREEN_WIDTH")) {
                    tryCompare(topBar.screen, "width", Number(Quickshell.env("BINGUX_TEST_SCREEN_WIDTH")), 3000);
                    tryCompare(topBar.screen, "height", Number(Quickshell.env("BINGUX_TEST_SCREEN_HEIGHT")), 3000);
                    console.log("VERIFIED_SCREEN", topBar.screen.width, topBar.screen.height, topBar.screen.devicePixelRatio);
                }
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                editor.open();
                for (const window of [dock, terminalSidebar.editWindow, controlCentre.nativeWindow, editor.nativeWindow])
                    tryVerify(() => window.screen.name === topBar.screen.name, 3000, "All real containers use the edited display");
                editor.change("sidebarEdge", "right");
                wait(400);
                const before = JSON.stringify(BinguxPreferences.data.desktop);
                const palette = findChild((editor.paletteNativeWindow || editor.nativeWindow).contentItem, "customisePalette");
                verify(palette.visible);
                verify(palette.width >= 240, "The palette has room for its search and previews");
                verify(palette.x >= 0 && palette.x + palette.width <= editor.width);
                verify(palette.y >= Theme.barHeight && palette.y + palette.height < editor.height - 56);
                const grid = findChild(palette, "customiseWidgetGrid");
                verify(grid.height >= 144, "A complete preview row fits without clipping");
                const toggle = findChild(editor.contentItem, "customisePaletteToggle");
                const done = findChild(editor.contentItem, "customiseApply");
                const donePoint = done.mapToItem(editor.contentItem, done.width, done.height);
                verify(donePoint.x <= editor.width - editor.rightInset, "Done remains reachable beside the sidebar");
                const tile = findChild(palette, "customise-widget-spacer");
                verify(tile !== null);
                const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "top-left");
                gesture(tile, editor.paletteNativeWindow, ["--drag-to", String(area.screenRect.x + 12), String(area.screenRect.y + 16)]);
                tryVerify(() => topBar.spacingWidgets.some(item => !item.flexible), 3000, "Native palette drag reaches the original top bar");
                if (editor.compactPalette) {
                    verify(!palette.visible, "The palette clears the containers during a drag");
                    verify(controlCentre.visible);
                }
                const spacer = topBar.spacingWidgets.find(item => !item.flexible);
                const remove = editor.preview.paletteRect;
                gesture(spacer, topBar, ["--drag-to", String(remove.x + remove.width / 2), String(remove.y + remove.height / 2)]);
                tryVerify(() => !topBar.spacingWidgets.some(item => !item.flexible), 3000, "The visible Remove target accepts native container drags");
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                gesture(clockPill, topBar, ["--drag-to", String(dockArea.screenRect.x + dockArea.screenRect.width / 2),
                    String(dockArea.screenRect.y + dockArea.screenRect.height / 2)]);
                tryVerify(() => editor.containerFor("clock") === "dock", 3000, "Native drag reaches the dock on the edited display");
                compare(clockPill.parent, dock.widgetHost);
                editor.undo();
                tryVerify(() => editor.containerFor("clock") === "top-center", 3000);
                if (editor.compactPalette) gesture(toggle, editor.nativeWindow, ["--click-only"]);
                tryCompare(palette, "visible", true);
                gesture(findChild(palette, "customise-tab-Apps"), editor.paletteNativeWindow, ["--click-only"]);
                tryCompare(editor, "tab", "Apps", 3000, "The reopened palette receives input above the real control centre");
                gesture(findChild(palette, "customise-tab-Widgets"), editor.paletteNativeWindow, ["--click-only"]);
                tryCompare(editor, "tab", "Widgets");
                gesture(palette, editor.paletteNativeWindow, ["--hover-only"]);
                // Inspector and palette must not compete for the same input area.
                editor.selectContainer("control-centre");
                tryCompare(findChild(editor.optionsContentItem, "customiseInspector"), "visible", true);
                if (editor.compactPalette) verify(!palette.visible);
                editor.optionsPage = "";
                for (const edge of ["left", "top", "right"]) {
                    editor.change("sidebarEdge", edge);
                    editor.paletteOpen = true;
                    wait(350);
                    const edgePalette = findChild(editor.paletteNativeWindow.contentItem, "customisePalette");
                    verify(edgePalette.visible);
                    verify(edgePalette.y + edgePalette.height <= editor.height - 56);
                    verify(findChild(edgePalette, "customiseWidgetGrid").height >= 144, "The preview grid fits beside a " + edge + " sidebar");
                }
                editor.cancel();
                compare(JSON.stringify(BinguxPreferences.data.desktop), before, "Cancel keeps the saved layout unchanged");
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                console.error("CONTAINER_SCREENS", JSON.stringify({bar: topBar.screen?.name, dock: dock.screen?.name,
                    sidebar: terminalSidebar.editWindow.screen?.name, controls: controlCentre.nativeWindow.screen?.name,
                    editor: desktopCustomiser.nativeWindow.screen?.name}));
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
