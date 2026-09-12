    QtObject {
        id: reloadAdapter
        property bool enabled: true
        property bool discovering: false
        property var devices: QtObject {
            property var values: []
        }
    }
    FileView {
        id: layoutReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_connectivity() {
            try {
                controlCentre.bluetoothAdapter = reloadAdapter;
                const bluetooth = controlCentre.movableWidgets.find(item => item.widgetId === "control-bluetooth");
                verify(BinguxPreferences.data.desktop.layout.sidebar.includes("control-bluetooth"));
                tryCompare(bluetooth, "parent", terminalSidebar.widgetHost, 3000);
                compare(bluetooth.displayedTitle, "Bluetooth devices");
                verify(bluetooth.showLabel && bluetooth.showIcon && !bluetooth.barLayout);
                desktopCustomiser.open();
                compare(desktopCustomiser.containerFor("control-bluetooth"), "sidebar");
                desktopCustomiser.cancel();
                compare(bluetooth.parent, terminalSidebar.widgetHost);
                layoutReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                layoutReport.setText("FAIL " + error.stack);
            }
        }
    }
