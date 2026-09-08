    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    FileView { id: actionReport; path: Quickshell.env("BINGUX_ACTION_REPORT"); watchChanges: true; printErrors: false; onFileChanged: reload() }
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
                const originalDesktopLayout = JSON.stringify(editor.layout);
                const header = findChild(controlCentre.body, "controlHeader");
                const settings = findChild(controlCentre.body, "controlSettings");
                const account = findChild(controlCentre.body, "controlUserAccount");
                account.imageSource = Quickshell.shellPath("action-avatar.svg");
                const avatar = findChild(account, "iconButtonImage");
                tryCompare(avatar, "status", Image.Ready, 3000);
                const output = findChild(controlCentre.body, "controlOutputRow");
                const microphone = findChild(controlCentre.body, "controlInputRow");
                drag(settings, 16, 16, DesktopEditing.point(account, controlCentre.nativeWindow, 1, 16));
                compare(editor.desktop.controlLayout.groups["controls-header"][0], "control-settings");
                verify(settings.x < account.x, "The native settings button follows the drop");
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                const dockRect = dockArea.screenRect;
                drag(settings, 16, 16, Qt.point(dockRect.x + 12, dockRect.y + 12));
                compare(settings.parent, dock.widgetHost, "The actual settings button moves to the dock");
                verify(!editor.desktop.controlLayout.groups["controls-header"].includes("control-settings"));
                editor.selectedContainer = "dock"; editor.containerDisplay("text");
                editor.selectedWidget = "control-settings"; editor.widgetOption("label", "Preferences");
                tryCompare(settings, "displayedLabel", "Preferences");
                verify(settings.customPresentation && !settings.presentation.showIcon && settings.presentation.showText);
                editor.widgetOption("display", "both"); editor.widgetOption("icon", "starred-symbolic");
                verify(settings.presentation.showIcon && settings.presentation.showText && settings.presentation.icon === "starred-symbolic");
                editor.widgetOption("display", "inherit");
                verify(!settings.presentation.showIcon && settings.presentation.showText, "A button can return to its container style");
                editor.widgetOption("label", ""); editor.widgetOption("icon", "");
                editor.containerDisplay("native");
                gesture(settings, dock, settings.width / 2, settings.height / 2, ["--drag-to",
                    DesktopEditing.point(account, controlCentre.nativeWindow, 1, 16).x.toString(),
                    DesktopEditing.point(account, controlCentre.nativeWindow, 1, 16).y.toString()]);
                compare(settings.parent, header, "Moving back restores the same native button");

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
                compare(JSON.stringify(BinguxPreferences.data.desktop.layout), originalDesktopLayout, "Cancel preserves desktop placements");
                editor.open();
                editor.change("controlLayout", JSON.parse(original));
                editor.put("control-volume", "control-centre", 1);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                compare(BinguxPreferences.data.desktop.controlLayout.groups["controls-audio"][0], "control-microphone");
                const lock = findChild(controlCentre.body, "controlLock");
                editor.open();
                editor.put("control-settings", "dock", 0);
                editor.put("control-account", "top-left", 0);
                editor.put("control-lock", "top-right", 0);
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                compare(settings.parent, dock.widgetHost);
                compare(account.parent, leftControls);
                compare(findChild(account, "iconButtonImage"), avatar);
                compare(avatar.status, Image.Ready, "The existing avatar remains loaded after moving windows");
                compare(lock.parent, rightControls);
                mouseClick(settings, settings.width / 2, settings.height / 2, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000);
                compare(widgetMenu.widgetId, "control-settings", "The moved action keeps its Shift-right-click menu");
                widgetMenu.visible = false; wait(300);
                gesture(settings, dock, settings.width / 2, settings.height / 2, ["--click-only"]);
                gesture(account, topBar, account.width / 2, account.height / 2, ["--click-only"]);
                gesture(lock, topBar, lock.width / 2, lock.height / 2, ["--click-only"]);
                tryVerify(() => actionReport.text().trim().split("\n").length === 3, 4000, "Moved buttons retain their command dispatch");
                const commands = actionReport.text().trim().split("\n").map(line => JSON.parse(line));
                compare(JSON.stringify(commands), JSON.stringify([
                    {command: "gnome-control-center", arguments: [""]},
                    {command: "gnome-control-center", arguments: ["users"]},
                    {command: "loginctl", arguments: ["lock-session"]}
                ]));
                const battery = findChild(controlCentre.body, "controlBattery");
                battery.available = false;
                editor.open();
                tryCompare(controlCentre, "visible", true, 3000);
                tryCompare(controlCentre, "revealScale", 1, 4000);
                wait(300);
                verify(battery.visible, "An absent battery remains editable in Customise UI");
                drag(battery, battery.width / 2, battery.height / 2,
                    Qt.point(dockArea.screenRect.x + 12, dockArea.screenRect.y + 12));
                compare(battery.parent, dock.widgetHost, "The original battery display moves into the dock");
                verify(!editor.desktop.controlLayout.groups["controls-header"].includes("control-battery"));
                editor.undo(); wait(150); compare(battery.parent, header);
                editor.redo(); wait(150); compare(battery.parent, dock.widgetHost);
                editor.selectedContainer = "dock"; editor.containerDisplay("text");
                editor.selectedWidget = "control-battery"; editor.widgetOption("label", "Charge");
                verify(battery.presentation.showText && !battery.presentation.showIcon);
                compare(battery.presentation.label, "Charge");
                editor.widgetOption("display", "both"); editor.widgetOption("icon", "starred-symbolic");
                verify(battery.presentation.showIcon && battery.presentation.icon === "starred-symbolic");
                editor.widgetOption("label", ""); editor.widgetOption("icon", ""); editor.widgetOption("display", "inherit");
                editor.containerDisplay("native");
                editor.apply(); tryCompare(editor, "visible", false, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                compare(BinguxPreferences.data.desktop.layout.dock[0], "control-battery");
                verify(!battery.visible, "Outside the editor an absent battery stays hidden");
                battery.summary = "Battery 84 percent, charging"; battery.available = true;
                tryCompare(battery, "visible", true, 1000); compare(battery.label, "84%");
                battery.summary = "Battery 79 percent, discharging";
                tryCompare(battery, "label", "79%", 1000);
                tryVerify(() => battery.width >= battery.implicitWidth && battery.height > 0, 1000);
                wait(200);
                mouseClick(battery, battery.width / 2, battery.height / 2, Qt.RightButton, Qt.ShiftModifier);
                tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "control-battery");
                widgetMenu.visible = false;
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
