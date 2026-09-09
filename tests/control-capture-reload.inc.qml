    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised && captureTool.ready
        function test_saved_capture() {
            try {
                verify(BinguxPreferences.data.desktop.layout.sidebar.includes("capture"));
                tryCompare(captureStatus, "parent", terminalSidebar.widgetHost, 3000);
                compare(captureStatus.presentation.label, "Capture status");
                compare(captureTool.state, "idle"); verify(!captureStatus.visible);
                desktopCustomiser.open();
                compare(desktopCustomiser.containerFor("capture"), "sidebar");
                desktopCustomiser.cancel();
                compare(captureStatus.parent, terminalSidebar.widgetHost);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
