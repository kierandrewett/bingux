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
        id: dockUnpinTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function actionWithText(item, text) {
            if (item.text === text && item.clicked) return item;
            for (const child of item.children || []) {
                const found = actionWithText(child, text);
                if (found) return found;
            }
            return null;
        }
        function gesture(item, window, x, y, options) {
            const point = DesktopEditing.point(item, window, x, y);
            nativeInput.gestureArguments = [point.x.toString(), point.y.toString()].concat(options);
            nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000);
            compare(nativeInput.resultCode, 0, "The compositor accepts the native gesture");
        }
        function test_direct_unpin() {
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
                tryVerify(() => editor.applications.length > 0);
                const appId = "app:" + editor.appId(editor.applications[0].id);
                editor.put(appId, "dock", 0); wait(400);
                const area = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                wait(350);
                const normalApp = area.entries.find(entry => entry.id === appId).item;
                gesture(normalApp, dock, normalApp.width / 2, normalApp.height / 2, ["--right-click"]);
                const normalMenu = findChild(normalApp, "dockAppMenu");
                tryCompare(normalMenu, "visible", true, 3000);
                tryCompare(normalMenu, "revealScale", 1, 3000);
                const unpin = findChild(normalMenu.body, "dockPinAction");
                compare(unpin.label, "Unpin from dock");
                const launchNew = actionWithText(normalMenu.body, "Open new window") || actionWithText(normalMenu.body, "Launch new...");
                verify(launchNew !== null && unpin.y < launchNew.y, "Unpin is grouped before Launch new");
                gesture(unpin, normalMenu.nativeWindow, unpin.width / 2, unpin.height / 2, ["--click-only"]);
                tryVerify(() => !dock.pinnedApps.includes(appId.slice(4)), 3000);
                tryVerify(() => !normalMenu?.visible, 3000, "The menu closes when the app is unpinned");
                // Wait for Settings to reload the file before opening another draft.
                binguxSettings.read();
                tryCompare(binguxSettings, "busy", false, 4000);
                verify(!binguxSettings.draft.desktop.dockApps.pinnedApps.includes(appId.slice(4)), "Normal unpin reaches the saved settings");
                editor.open(); editor.put(appId, "dock", 0); wait(400);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                wait(350);
                const shiftApp = area.entries.find(entry => entry.id === appId).item;
                gesture(shiftApp, dock, shiftApp.width / 2, shiftApp.height / 2, ["--shift-right-click"]);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.menuEntries[0].text, "Unpin from dock");
                const shiftUnpin = actionWithText(widgetMenu.body, "Unpin from dock");
                verify(shiftUnpin !== null);
                gesture(shiftUnpin, widgetMenu.nativeWindow, shiftUnpin.width / 2, shiftUnpin.height / 2, ["--click-only"]);
                tryVerify(() => !dock.pinnedApps.includes(appId.slice(4)), 3000);
                verify(!editor.visible, "Shift-right-click unpins without entering Customise UI");
                binguxSettings.read();
                tryCompare(binguxSettings, "busy", false, 4000);
                verify(!binguxSettings.draft.desktop.dockApps.pinnedApps.includes(appId.slice(4)), "Shift unpin reaches the saved settings");
                editor.open(); editor.put(appId, "dock", 0); wait(400);
                const app = area.entries.find(entry => entry.id === appId).item;
                gesture(app, dock, app.width / 2, app.height / 2, ["--right-click"]);
                const menu = findChild(area, "customiseAppActions");
                tryCompare(menu, "visible", true);
                compare(menu.menuEntries[0].text, "Unpin from dock");
                verify(menu.menuEntries[0].enabled);
                const action = actionWithText(menu.body, "Unpin from dock");
                verify(action !== null);
                gesture(action, menu.nativeWindow, action.width / 2, action.height / 2, ["--click-only"]);
                tryVerify(() => !editor.dockApplications.includes(appId));
                verify(editor.optionsPage === "", "Unpin does not open the options panel");
                editor.undo(); verify(editor.dockApplications.includes(appId));
                editor.redo(); verify(!editor.dockApplications.includes(appId));
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                verify(!BinguxPreferences.data.desktop.dockApps.pinnedApps.includes(appId.slice(4)), "Unpin is saved");
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
