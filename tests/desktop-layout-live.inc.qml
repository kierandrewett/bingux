    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeTyping
        property point position
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT"), position.x.toString(), position.y.toString()]

    }
    Component { id: externalSettingsComponent; BinguxSettings {} }
    TestCase {
        id: layoutTest
        parent: binguxSettings.customiser.contentItem
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
            mouseClick(systemPill, systemPill.width / 2, systemPill.height / 2, Qt.RightButton, Qt.ShiftModifier);
            tryCompare(widgetMenu, 'visible', true);
            compare(widgetMenu.widgetId, 'controls');
            verify(!controlCentre.visible, 'Shift-right-click does not activate the control');
            widgetMenu.activate(widgetMenu.actions[0]);
            tryCompare(binguxSettings.customiser, 'visible', true, 4000);
            compare(binguxSettings.customiser.selectedWidget, 'controls');
            compare(binguxSettings.customiser.optionsPage, 'Widget');
            binguxSettings.customiser.cancel();
            tryCompare(binguxSettings, 'ready', true, 4000);
            tryCompare(binguxSettings, 'busy', false, 4000);
            const editor = binguxSettings.customiser;
            editor.open();
            tryCompare(controlCentre, 'visible', true, 2000);
            compare(searchPill.parent, leftControls, 'Entering edit mode keeps the original widget instance in its native container');
            compare(controlCentre.hostItem, null, 'Control centre stays in its native window');
            wait(1000);
            tryVerify(() => !!DesktopEditing.previews.search && !!DesktopEditing.previews['control-network'], 3000, 'Palette uses captures of the actual widgets');
            // Reopening keeps the same QQuickWindow even after preview controls exist.
            const editWindow = editor.nativeWindow.contentItem.Window.window;
            for (let cycle = 0; cycle < 3; cycle++) {
                editor.optionsPage = 'Widget'; editor.selectedWidget = 'search';
                wait(80); editor.cancel(); wait(80); editor.open(); wait(100);
                compare(editor.nativeWindow.contentItem.Window.window, editWindow);
            }
            editor.optionsPage = 'Widget'; editor.selectedWidget = 'search';
            wait(100);
            const label = findChild(editor.contentItem, 'customiseWidgetLabelInput');
            verify(label);
            nativeTyping.position = DesktopEditing.point(label, editor.nativeWindow, 20, 12);
            nativeTyping.running = true;
            tryCompare(label, "text", "Find", 4000);
            wait(100); // Release the last native key before testing another control.
            const star = findChild(editor.contentItem, 'customise-icon-starred-symbolic');
            mouseClick(star, 18, 18);
            compare(editor.desktop.widgetOptions.search.label, 'Find');
            compare(editor.desktop.widgetOptions.search.icon, 'starred-symbolic');
            const dockEdit = DesktopEditing.surfaces.find(area => area.zoneName === 'dock');
            mouseClick(dockEdit, 4, 4);
            compare(editor.selectedContainer, 'dock');
            compare(editor.optionsPage, 'Container', 'Clicking the actual dock opens its settings');
            editor.optionsPage = '';
            const searchTile = findChild(editor.contentItem, 'customise-widget-search');
            verify(searchTile);
            const destination = dockEdit.screenRect;
            const tilePoint = searchTile.mapToItem(editor.preview, 0, 0);
            mousePress(searchTile, 20, 20);
            mouseMove(searchTile, 38, 20, 50);
            mouseMove(searchTile, destination.x + 12 - tilePoint.x, destination.y + 12 - tilePoint.y, 150);
            mouseRelease(searchTile, destination.x + 12 - tilePoint.x, destination.y + 12 - tilePoint.y);
            compare(editor.layout.dock[0], 'search', 'Palette drag targets the real dock');
            compare(searchPill.parent, dock.widgetHost, 'The existing widget moves immediately in preview');
            verify(!BinguxPreferences.data.desktop.layout.dock.includes('search'), 'Preview is not written to settings');
            editor.change('sidebarEdge', 'left');
            compare(terminalSidebar.edge, 'left', 'Sidebar edge previews without writing its old settings');
            editor.cancel();
            compare(terminalSidebar.edge, initial.sidebar.edge, 'Cancel restores the actual sidebar edge');
            tryCompare(searchPill, 'parent', leftControls);
            editor.open();
            wait(300);
            editor.optionsPage = ''; editor.tab = 'Apps';
            tryVerify(() => editor.applications.length > 0);
            const appId = editor.appId(editor.applications[0].id);
            wait(100);
            const appTile = findChild(editor.contentItem, 'customise-widget-app:' + appId);
            verify(appTile);
            const appPoint = appTile.mapToItem(editor.preview, 0, 0);
            const dockPoint = dockEdit.screenRect;
            mousePress(appTile, 20, 20);
            mouseMove(appTile, 38, 20, 50);
            mouseMove(appTile, dockPoint.x + 12 - appPoint.x, dockPoint.y + 12 - appPoint.y, 150);
            mouseRelease(appTile, dockPoint.x + 12 - appPoint.x, dockPoint.y + 12 - appPoint.y);
            compare(editor.desktop.dockApps.pinnedApps[0], appId, 'Dragging an app pins it in the live dock preview');
            editor.tab = 'Widgets';
            const ccEdit = DesktopEditing.surfaces.find(area => area.zoneName === 'control-centre');
            const network = ccEdit.entries.find(entry => entry.id === 'control-network').item;
            const bluetooth = ccEdit.entries.find(entry => entry.id === 'control-bluetooth').item;
            const networkPoint = network.mapToItem(ccEdit, 20, 20);
            const bluetoothPoint = bluetooth.mapToItem(ccEdit, bluetooth.width - 2, bluetooth.height / 2);
            mousePress(ccEdit, networkPoint.x, networkPoint.y);
            mouseMove(ccEdit, networkPoint.x + 12, networkPoint.y, 50);
            mouseMove(ccEdit, bluetoothPoint.x, bluetoothPoint.y, 150);
            mouseRelease(ccEdit, bluetoothPoint.x, bluetoothPoint.y);
            compare(editor.desktop.controlOrder[0], 'bluetooth', 'Dragging the actual network tile reorders the control centre');
            const reorderedControls = editor.desktop.controlOrder.slice();
            const originalControls = editor.desktop.controlCentre;
            editor.change('controlCentre', Object.assign({}, originalControls, {nightLight: false, power: false}));
            editor.change('controlOrder', ['nightLight', 'network', 'power', 'bluetooth', 'vpn', 'dnd', 'awake']);
            wait(200);
            verify(!controlCentre.controlVisible('nightLight') && !controlCentre.controlVisible('power'));
            const networkStart = network.mapToItem(ccEdit, 20, 20);
            const bluetoothEnd = bluetooth.mapToItem(ccEdit, bluetooth.width - 2, bluetooth.height / 2);
            mousePress(ccEdit, networkStart.x, networkStart.y);
            mouseMove(ccEdit, networkStart.x + 12, networkStart.y, 50);
            mouseMove(ccEdit, bluetoothEnd.x, bluetoothEnd.y, 150);
            mouseRelease(ccEdit, bluetoothEnd.x, bluetoothEnd.y);
            compare(JSON.stringify(editor.desktop.controlOrder.slice(0, 4)), JSON.stringify(['nightLight', 'power', 'bluetooth', 'network']), 'Hidden tiles do not shift the visible drop position');
            wait(100);
            compare(controlCentre.controlCell('bluetooth').column, 0);
            compare(controlCentre.controlCell('network').column, 1);
            editor.change('controlOrder', reorderedControls);
            editor.change('controlCentre', originalControls);
            const done = findChild(editor.contentItem, 'customiseApply');
            const donePoint = done.mapToItem(editor.preview, done.width, done.height);
            verify(donePoint.x <= editor.width - editor.rightInset, 'Done remains outside the sidebar');
            editor.put('search', 'top-right', 0);
            editor.put('clock', 'dock', 0);
            editor.put('controls', 'dock', 1);
            editor.put('metrics', 'top-left', 0);
            editor.put('notes', 'sidebar', 0);
            editor.put('control-bluetooth', 'control-centre', 0);
            editor.selectedContainer = 'dock'; editor.containerDisplay('text');
            editor.selectedWidget = 'clock'; editor.widgetOption('display', 'both'); editor.widgetOption('label', 'Clock'); editor.widgetOption('icon', 'starred-symbolic');
            editor.change('sidebarEdge', 'left');
            editor.change('dockSize', 40);
            editor.change('dockAlignment', 'left');
            mouseClick(done, done.width / 2, done.height / 2);
            tryCompare(binguxSettings, 'busy', false, 4000);
            tryCompare(editor, 'visible', false, 2000);
            tryCompare(clockPill, 'parent', dock.widgetHost);
            compare(systemPill.parent, dock.widgetHost);
            compare(systemPill.presentation.showIcon, false);
            compare(systemPill.presentation.showText, true);
            compare(clockPill.presentation.label, 'Clock');
            compare(clockPill.presentation.icon, 'starred-symbolic');
            compare(clockPill.presentation.showIcon, true);
            tryVerify(() => clockPill.x < systemPill.x, 2000, 'Dock widgets keep the configured order');
            compare(metricsPill.parent, leftControls);
            compare(searchPill.parent, rightControls);
            compare(dock.iconSize, 40);
            compare(terminalSidebar.edge, 'left', 'Saved sidebar edge remains applied after leaving the editor');
            compare(terminalSidebar.contentTypes[0].id, 'notes');
            tryVerify(() => dock.widgetHost.implicitWidth > 0, 2000);
            controlCentre.visible = true;
            wait(400);
            compare(controlCentre.controlCell('bluetooth').column, 0, 'The live control centre follows the saved tile order');
            compare(controlCentre.controlCell('network').column, 1);
            const networkWidget = findChild(controlCentre.contentItem, 'controlNetwork');
            mouseClick(networkWidget, 20, 20, Qt.RightButton, Qt.ShiftModifier);
            tryCompare(widgetMenu, 'visible', true);
            compare(widgetMenu.widgetId, 'control-network');
            widgetMenu.visible = false;
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
            const externalSettings = externalSettingsComponent.createObject(root);
            externalSettings.customiser.open();
            tryCompare(editor, 'visible', true, 5000, 'Standalone Settings opens the actual shell editor over IPC');
            verify(!externalSettings.customiser.initialised, 'Standalone Settings does not create another editor window');
            editor.cancel();
            externalSettings.destroy();
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
