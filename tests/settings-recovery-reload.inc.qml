    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_retry() {
            try {
                verify(BinguxPreferences.data.desktop.layout.dock.includes("clock"));
                tryCompare(clockPill, "parent", dock.widgetHost, 3000);
                compare(BinguxPreferences.layoutError, "");
                desktopCustomiser.open();
                desktopCustomiser.cancel();
                compare(clockPill.parent, dock.widgetHost);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
