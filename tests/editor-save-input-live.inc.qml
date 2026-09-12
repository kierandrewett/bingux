    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    Process {
        id: savedInput
        property point position
        property int resultCode: -1
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT"), String(position.x), String(position.y), "--click-only"]
        onExited: (code, status) => resultCode = code
    }
    SignalSpy {
        id: networkRequests
        target: DesktopEditing.sources["control-network"] || null
        signalName: "navigationRequested"
    }
    TestCase {
        id: savedInputTest
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_click_after_save() {
            try {
                BinguxPreferences.importDesktop(root.layoutSnapshot());
                tryVerify(() => !!BinguxPreferences.data.desktop.controlLayout, 4000);
                binguxSettings.read();
                tryCompare(binguxSettings, "busy", false, 4000);
                terminalSidebar.open();
                const editor = desktopCustomiser;
                const network = DesktopEditing.sources["control-network"];
                for (let cycle = 0; cycle < 12; cycle++) {
                    editor.open();
                    editor.put("search", "top-right", 0);
                    editor.put("clock", "dock", 0);
                    editor.put("controls", "dock", 1);
                    editor.put("metrics", "top-left", 0);
                    editor.put("control-network", "top-left", 1);
                    editor.put(editor.layout["top-left"].find(id => id.startsWith("spring:")) || "spring", "top-left", 1);
                    editor.change("sidebarEdge", cycle % 2 ? "right" : "left");
                    editor.apply();
                    tryCompare(binguxSettings, "busy", false, 4000);
                    tryCompare(editor, "visible", false, 2000);
                    compare(network.parent, leftControls);
                    root.openWidgetMenu("control-network", network, topBar);
                    tryCompare(widgetMenu, "visible", true);
                    widgetMenu.visible = false;
                    tryCompare(widgetMenu, "retained", false);
                    tryCompare(terminalSidebar.editWindow, "reveal", 1, 3000);
                    tryVerify(() => topBar.width === topBar.screen.width - topBar.margins.left - topBar.margins.right, 3000, "The compositor acknowledges the bar resize before clicking");
                    verify(waitForRendering(network));
                    networkRequests.clear();
                    savedInput.position = DesktopEditing.point(network, topBar, network.width / 2, network.height / 2);
                    savedInput.running = true;
                    tryCompare(savedInput, "running", false, 4000);
                    compare(savedInput.resultCode, 0);
                    compare(networkRequests.count, 1, "The saved control receives one native click");
                    compare(controlCentre.visible, true);
                    compare(controlCentre.detailOpen, true);
                    compare(controlCentre.detailPage, "network");
                    controlCentre.visible = false;
                    tryCompare(controlCentre, "retained", false);
                    binguxSettings.read();
                    tryCompare(binguxSettings, "busy", false, 4000);
                }
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
