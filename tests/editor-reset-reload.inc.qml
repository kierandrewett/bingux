    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_reset_reload() {
            try {
                compare(JSON.stringify(topBar.snapshotLayout()), JSON.stringify(DesktopLayout.defaults()));
                compare(JSON.stringify(BinguxPreferences.data.desktop.widgetOptions), "{}");
                compare(JSON.stringify(BinguxPreferences.data.desktop.containers), "{}");
                compare(BinguxPreferences.data.desktop.dockSize, 56);
                compare(terminalSidebar.edge, "right");
                verify(BinguxPreferences.data.desktop.dockApps.pinnedApps.length > 0);
                compare(clockPill.parent, centerControls);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
