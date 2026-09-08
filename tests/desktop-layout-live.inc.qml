    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    TestCase {
        id: layoutTest
        when: topBar.visible
        function test_live_layout() {
            try {
            compare(searchPill.parent, leftControls);
            compare(clockPill.parent, centerControls);
            tryCompare(BinguxPreferences, 'loaded', true);
            tryCompare(ControlCentreServices, 'preferencesReady', true);
            tryCompare(dock, 'appGroupsInitialised', true);
            const initial = root.layoutSnapshot();
            BinguxPreferences.importDesktop(initial);
            tryVerify(() => BinguxPreferences.data.desktop.layoutVersion === 1, 4000);
            compare(JSON.stringify(topBar.snapshotLayout()), JSON.stringify(initial.layout));
            compare(JSON.stringify(dock.snapshotLayout()), JSON.stringify(initial.dock));
            compare(JSON.stringify(ControlCentreServices.effectiveControls), JSON.stringify(initial.controlCentre));
            compare(terminalSidebar.edge, initial.sidebar.edge);
            binguxSettings.visible = true;
            tryCompare(binguxSettings, 'ready', true, 4000);
            tryCompare(binguxSettings, 'busy', false, 4000);
            const editor = binguxSettings.customiser;
            editor.open();
            editor.put('search', 'top-right', 0);
            editor.put('clock', 'dock', 0);
            editor.put('controls', 'dock', 1);
            editor.put('metrics', 'top-left', 0);
            editor.put('notes', 'sidebar', 0);
            editor.change('dockSize', 40);
            editor.change('dockAlignment', 'left');
            editor.apply();
            tryCompare(binguxSettings, 'busy', false, 4000);
            tryCompare(editor, 'visible', false, 2000);
            tryCompare(clockPill, 'parent', dock.widgetHost);
            compare(systemPill.parent, dock.widgetHost);
            tryVerify(() => clockPill.x < systemPill.x, 2000, 'Dock widgets keep the configured order');
            compare(metricsPill.parent, leftControls);
            compare(searchPill.parent, rightControls);
            compare(dock.iconSize, 40);
            compare(terminalSidebar.contentTypes[0].id, 'notes');
            tryVerify(() => dock.widgetHost.implicitWidth > 0, 2000);
            controlCentre.visible = true;
            wait(400);
            compare(controlCentre.anchorWindow, dock);
            verify(controlCentre.preferredY > Theme.barHeight, 'Dock popup is placed above the dock');
            verify(controlCentre.preferredY + controlCentre.popupHeight <= dock.popupAnchorTop - Theme.gap + 1, 'Dock popup clears the dock');
            controlCentre.visible = false;
            const layout = JSON.parse(JSON.stringify(topBar.customLayout));
            layout['top-left'] = ['search']; layout['top-right'] = layout['top-right'].filter(id => id !== 'search');
            layout['dock'] = layout['dock'].concat(['metrics']);
            BinguxPreferences.saveLayout(layout);
            tryCompare(searchPill, 'parent', leftControls);
            tryCompare(metricsPill, 'parent', dock.widgetHost);
            wait(400);
            binguxSettings.read(); tryCompare(binguxSettings, 'busy', false, 4000);
            compare(binguxSettings.draft.desktop.layout['top-left'][0], 'search');
            // A Settings draft can outlive a change made directly in the dock.
            BinguxPreferences.saveDesktop({dockApps: {pinnedApps: ['org.gnome.Nautilus'], order: ['org.gnome.Nautilus']}});
            wait(300);
            binguxSettings.update('previews', 'maxMegabytes', 5);
            binguxSettings.save();
            tryCompare(binguxSettings, 'busy', false, 4000);
            compare(BinguxPreferences.data.desktop.dockApps.pinnedApps[0], 'org.gnome.Nautilus', 'Saving unrelated settings preserves new dock pins');
            const crowded = DesktopLayout.defaults();
            crowded['top-left'] = crowded['top-left'].concat(crowded['top-center'], crowded['top-right']);
            crowded['top-center'] = []; crowded['top-right'] = [];
            BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {desktop: Object.assign({}, BinguxPreferences.data.desktop, {layout: crowded})});
            topBar.margins.right = topBar.screen.width - 300;
            tryVerify(() => topBar.overflowItems.length > 0, 2000);
            tryVerify(() => overflowButton.visible, 2000);
            verify(topBar.overflowItems.every(item => item.parent === overflowColumn));
            console.log('DESKTOP_LAYOUT_LIVE_PASS');
            layoutReport.setText('PASS');
            } catch (error) { console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack); layoutReport.setText("FAIL " + error.stack); }
        }
    }
