    FileView {
        id: groupReloadReport
        path: Quickshell.env("BINGUX_LAYOUT_REPORT")
    }
    TestCase {
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function test_saved_groups() {
            try {
                binguxSettings.read();
                tryCompare(binguxSettings, "ready", true, 4000);
                tryCompare(binguxSettings, "busy", false, 4000);
                compare(BinguxPreferences.data.desktop.layoutVersion, 1);
                const cases = [
                    {
                        id: "controls-header",
                        zone: "top-left",
                        child: "control-settings"
                    },
                    {
                        id: "controls-audio",
                        zone: "dock",
                        child: "control-volume"
                    },
                    {
                        id: "controls-tiles",
                        zone: "top-center",
                        child: "control-network"
                    }
                ];
                for (const entry of cases) {
                    const group = controlCentre.movableWidgets.find(item => item.widgetId === entry.id);
                    const child = controlCentre.movableWidgets.find(item => item.widgetId === entry.child);
                    verify(BinguxPreferences.data.desktop.layout[entry.zone].includes(entry.id));
                    verify(!BinguxPreferences.data.desktop.controlLayout.groups["control-centre"].includes(entry.id));
                    tryCompare(group, "parent", topBar.hostFor(group), 2000);
                    compare(child.parent, group);
                    verify(group.barLayout && child.barLayout && !child.placed);
                    compare(child.barWindow, group.barWindow);
                }
                const settings = controlCentre.movableWidgets.find(item => item.widgetId === "control-settings");
                compare(settings.displayedLabel, "Preferences");
                verify(settings.presentation.showText && settings.presentation.showIcon);
                compare(BinguxPreferences.data.desktop.containers["controls-header"].display, "text");
                desktopCustomiser.open();
                compare(JSON.stringify(desktopCustomiser.layout), JSON.stringify(BinguxPreferences.data.desktop.layout));
                desktopCustomiser.cancel();
                groupReloadReport.setText("PASS");
            } catch (error) {
                console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack);
                groupReloadReport.setText("FAIL " + error.stack);
            }
        }
    }
