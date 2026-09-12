    FileView {
        id: report
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_monitors() {
            try {
                metricsPill.preferencesLocation = Qt.resolvedUrl("metrics-panel.ini");
                tryCompare(metricsPill, "parent", controlCentre.widgetHost, 3000);
                verify(metricsPill.panelLayout);
                tryVerify(() => metricsPill.selectedNames.length === 9, 3000);
                controlCentre.visible = true;
                tryVerify(() => metricsPill.width <= controlCentre.widgetHost.width, 3000);
                verify(metricsPill.height > Theme.barHeight);
                report.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                report.setText("FAIL " + error.stack);
            }
        }
    }
