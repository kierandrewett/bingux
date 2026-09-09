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
        function test_native_utilities() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                const editor = desktopCustomiser;
                const original = JSON.stringify(BinguxPreferences.data.desktop);
                editor.open(); tryCompare(controlCentre, "visible", true, 3000); tryCompare(controlCentre, "revealScale", 1, 4000); tryCompare(dock.margins, "bottom", 64, 2000); wait(300);
                const divider = findChild(controlCentre.body, "controlDivider");
                const space = findChild(controlCentre.body, "controlHeaderSpace");
                const button = findChild(controlCentre.body, "controlCustomise");
                const nativeDividerParent = divider.parent;
                const nativeSpaceParent = space.parent;
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                const top = DesktopEditing.surfaces.find(surface => surface.zoneName === "top-left");
                compare(divider.height, 1); compare(button.height, 28); verify(!button.showIcon && button.showLabel);
                const centreSurface = DesktopEditing.surfaces.find(surface => surface.zoneName === "control-centre");
                const hit = divider.mapToItem(centreSurface, divider.width / 2, 0.5);
                compare(centreSurface.entryAt(hit.x, hit.y)?.id, divider.widgetId);
                drag(divider, controlCentre.nativeWindow, divider.width / 2, 0.5,
                    Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                verify(divider.parent === dock.widgetHost, "Divider drop: " + JSON.stringify(editor.layout)); compare(divider.width, 1); compare(divider.height, Theme.barHeight - 12);
                editor.undo(); wait(100); verify(divider.parent === nativeDividerParent); compare(divider.height, 1);
                editor.redo(); wait(100); verify(divider.parent === dock.widgetHost);
                drag(space, controlCentre.nativeWindow, space.width / 2, space.height / 2, Qt.point(top.screenRect.x + 10, top.screenRect.y + 16));
                verify(space.parent === leftControls); compare(space.width, 16); compare(space.height, Theme.barHeight);
                editor.selectedWidget = space.widgetId; editor.widgetOption("width", 48); wait(100); compare(space.width, 48);
                editor.put(space.widgetId, "control-centre", 1); wait(100); verify(space.parent === nativeSpaceParent); compare(space.width, 48);
                editor.widgetOption("width", undefined); wait(100); verify(!space.fixedWidth && space.Layout.fillWidth);
                editor.put(space.widgetId, "top-left", 0); editor.widgetOption("width", 48); wait(100);
                drag(space, topBar, 24, 16, Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                verify(space.parent === dock.widgetHost); compare(space.width, 48);
                editor.put("control-customise", "top-left", 0); wait(100);
                verify(button.parent === leftControls); verify(button.showIcon && !button.showLabel); compare(button.height, Theme.barHeight);
                editor.selectedContainer = "top-left"; editor.containerDisplay("text"); verify(!button.showIcon && button.showLabel);
                editor.selectedWidget = button.widgetId; editor.widgetOption("label", "Edit desktop"); editor.widgetOption("display", "both"); editor.widgetOption("icon", "starred-symbolic");
                verify(button.showLabel && button.showIcon); compare(button.displayedText, "Edit desktop"); compare(button.displayedIcon, "starred-symbolic");
                editor.cancel(); wait(200);
                compare(JSON.stringify(BinguxPreferences.data.desktop), original);
                verify(divider.parent === nativeDividerParent && space.parent === nativeSpaceParent && button.parent === nativeDividerParent);
                editor.open();
                editor.put("control-divider", "dock", 0);
                editor.put("control-header-space", "dock", 1);
                editor.put("control-customise", "top-left", 0);
                editor.selectedWidget = space.widgetId; editor.widgetOption("width", 48);
                editor.selectedWidget = button.widgetId; editor.widgetOption("label", "Edit desktop"); editor.widgetOption("display", "both");
                for (const item of [divider, space, button]) {
                    const sample = sampleComponent.createObject(editor.contentItem, {widgetId: item.widgetId});
                    tryVerify(() => sample.previewControl !== null, 1500);
                    compare(sample.previewControl.implicitWidth, item.implicitWidth);
                    compare(sample.previewControl.implicitHeight, item.implicitHeight);
                    sample.destroy();
                }
                capture("utilities-editor", button, topBar);
                save();
                capture("utilities", button, topBar);
                gesture(space, dock, space.width / 2, space.height / 2, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, space.widgetId); verify(widgetMenu.anchorWindow === dock);
                widgetMenu.visible = false; wait(300);
                gesture(button, topBar, button.width / 2, button.height / 2, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, button.widgetId);
                widgetMenu.visible = false; wait(300);
                gesture(button, topBar, button.width / 2, button.height / 2, ["--click-only"]);
                tryCompare(editor, "visible", true, 4000);
                editor.cancel();
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack, binguxSettings.status);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
