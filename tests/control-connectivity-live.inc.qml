    QtObject {
        id: testHeadphones
        property string name: "Test headphones"
        property string deviceName: name
        property string icon: "audio-headphones"
        property bool connected: false
        property bool paired: true
        property int state: connected ? BluetoothDeviceState.Connected : BluetoothDeviceState.Disconnected
        property int connections: 0
        property int disconnections: 0
        function connect() { connections++; connected = true; }
        function disconnect() { disconnections++; connected = false; }
    }
    QtObject {
        id: testAdapter
        property bool enabled: true
        property bool discovering: false
        property var devices: QtObject { property var values: [testHeadphones] }
    }
    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeInput
        property int resultCode: -1
        property var gestureArguments: []
        onExited: code => resultCode = code
        stderr: StdioCollector { onStreamFinished: if (text) console.warn("NATIVE_INPUT", text) }
        property string capturePath: ""
        command: ["env", "BINGUX_NATIVE_SCREENSHOT=" + capturePath, "python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT")].concat(gestureArguments)
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function gesture(item, window, options) {
            if (window === controlCentre.nativeWindow && controlCentre.detailOpen)
                tryVerify(() => Math.abs(controlCentre.controlsHeight - Math.min(controlCentre.deviceControls.implicitHeight, Math.max(0, controlCentre.maximumPopupHeight - controlCentre.contentPadding * 2))) < 0.1, 2000);
            verify(waitForPolish(window.contentItem.Window.window, 2000), "Finish layout before measuring native input coordinates");
            const point = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
            nativeInput.gestureArguments = [String(point.x), String(point.y)].concat(options || ["--click-only"]);
            nativeInput.resultCode = -1; nativeInput.running = true;
            tryCompare(nativeInput, "running", false, 4000); compare(nativeInput.resultCode, 0);
        }
        function deviceRow(item) {
            if (item.title === testHeadphones.name && item.clicked) return item;
            for (const child of item.children || []) {
                const row = deviceRow(child);
                if (row) return row;
            }
            return null;
        }
        function save() {
            desktopCustomiser.apply();
            tryCompare(desktopCustomiser, "visible", false, 4000);
            tryCompare(binguxSettings, "busy", false, 4000);
            tryCompare(dock.margins, "bottom", 0, 2000); wait(150);
        }
        function test_moved_bluetooth() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read(); tryCompare(binguxSettings, "ready", true, 4000); tryCompare(binguxSettings, "busy", false, 4000);
                controlCentre.bluetoothAdapter = testAdapter;
                const editor = desktopCustomiser;
                const bluetooth = controlCentre.movableWidgets.find(item => item.widgetId === "control-bluetooth");
                const originalParent = bluetooth.parent;
                editor.open(); tryCompare(controlCentre, "revealScale", 1, 3000); wait(250);
                const dockArea = DesktopEditing.surfaces.find(surface => surface.zoneName === "dock");
                gesture(bluetooth, controlCentre.nativeWindow, ["--hover-only"]);
                gesture(bluetooth, controlCentre.nativeWindow, ["--drag-to", String(dockArea.screenRect.x + dockArea.screenRect.width / 2), String(dockArea.screenRect.y + 16)]);
                tryCompare(editor, "draggedId", "", 3000);
                tryCompare(bluetooth, "parent", dock.widgetHost, 3000);
                verify(testAdapter.enabled, "Editing does not toggle the radio");
                editor.undo(); tryCompare(bluetooth, "parent", originalParent);
                editor.redo(); tryCompare(bluetooth, "parent", dock.widgetHost);
                editor.selectedWidget = "control-bluetooth";
                editor.widgetOption("display", "both"); editor.widgetOption("label", "Bluetooth devices");
                for (const zone of ["dock", "top-left", "sidebar"]) {
                    if (!editor.visible) editor.open();
                    editor.put("control-bluetooth", zone, 0);
                    save();
                    compare(controlCentre.movableWidgets.find(item => item.widgetId === "control-bluetooth"), bluetooth);
                    compare(bluetooth.parent, topBar.hostFor(bluetooth));
                    compare(bluetooth.displayedTitle, "Bluetooth devices");
                    verify(bluetooth.showIcon && bluetooth.showLabel);
                    if (zone === "sidebar") {
                        terminalSidebar.open(); tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                        const inlineSwitch = findChild(bluetooth, "controlBluetoothSwitch");
                        gesture(inlineSwitch, terminalSidebar.widgetWindow);
                        tryCompare(testAdapter, "enabled", false); verify(!controlCentre.visible);
                        gesture(inlineSwitch, terminalSidebar.widgetWindow);
                        tryCompare(testAdapter, "enabled", true);
                    }
                    const host = topBar.windowFor(bluetooth);
                    const launcher = bluetooth.barLayout ? bluetooth : findChild(bluetooth, "controlBluetoothNavigation");
                    gesture(launcher, host);
                    tryCompare(controlCentre, "visible", true, 3000);
                    tryCompare(controlCentre, "detailOpen", true, 3000);
                    compare(controlCentre.detailPage, "bluetooth");
                    compare(controlCentre.anchorWindow, host);
                    compare(controlCentre.movedAnchor, bluetooth);
                    tryCompare(controlCentre, "revealScale", 1, 3000);
                    tryCompare(testAdapter, "discovering", true, 3000);
                    tryCompare(controlCentre, "detailProgress", 1, 3000); wait(200);
                    verify(controlCentre.panelX >= 0 && controlCentre.panelY >= 0);
                    verify(controlCentre.panelX + controlCentre.body.parent.width <= topBar.screen.width);
                    verify(controlCentre.panelY + controlCentre.body.parent.height <= topBar.screen.height);
                    const power = findChild(controlCentre.deviceControls, "controlBluetoothPower");
                    const capturePrefix = Quickshell.env("BINGUX_CONNECTIVITY_CAPTURE");
                    if (capturePrefix) {
                        nativeInput.capturePath = capturePrefix + "-" + zone + ".png";
                        gesture(power, controlCentre.nativeWindow, ["--capture-only"]);
                        nativeInput.capturePath = "";
                    }
                    gesture(power, controlCentre.nativeWindow);
                    tryCompare(testAdapter, "enabled", false); tryCompare(testAdapter, "discovering", false);
                    gesture(power, controlCentre.nativeWindow);
                    tryCompare(testAdapter, "enabled", true); tryCompare(testAdapter, "discovering", true);
                    const row = deviceRow(controlCentre.deviceControls);
                    verify(row !== null);
                    const connections = testHeadphones.connections;
                    gesture(row, controlCentre.nativeWindow);
                    tryCompare(testHeadphones, "connections", connections + 1);
                    verify(testHeadphones.connected);
                    gesture(row, controlCentre.nativeWindow);
                    tryCompare(testHeadphones, "disconnections", connections + 1);
                    verify(!testHeadphones.connected);
                    controlCentre.visible = false;
                    tryCompare(controlCentre, "retained", false, 3000);
                    tryCompare(testAdapter, "discovering", false, 3000);
                    gesture(bluetooth, host, ["--shift-right-click"]);
                    tryCompare(widgetMenu, "visible", true, 3000); compare(widgetMenu.widgetId, "control-bluetooth");
                    widgetMenu.visible = false; tryCompare(widgetMenu, "retained", false, 3000);
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
