    FileView { id: utilityReloadReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_utilities() {
            try {
                const divider = controlCentre.movableWidgets.find(item => item.widgetId === "control-divider");
                const space = controlCentre.movableWidgets.find(item => item.widgetId === "control-header-space");
                const button = controlCentre.movableWidgets.find(item => item.widgetId === "control-customise");
                tryVerify(() => divider.parent === dock.widgetHost && space.parent === dock.widgetHost && button.parent === leftControls, 2000);
                compare(divider.width, 1); compare(divider.height, Theme.barHeight - 12); compare(space.width, 48);
                compare(button.displayedText, "Edit desktop"); verify(button.showIcon && button.showLabel);
                verify(!BinguxPreferences.data.desktop.controlLayout.groups["control-centre"].includes(divider.widgetId));
                verify(!BinguxPreferences.data.desktop.controlLayout.groups["controls-header"].includes(space.widgetId));
                utilityReloadReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                utilityReloadReport.setText("FAIL " + error.stack);
            }
        }
    }
