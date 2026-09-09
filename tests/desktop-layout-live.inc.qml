    FileView { id: layoutReport; path: Quickshell.env("BINGUX_LAYOUT_REPORT") }
    Process {
        id: nativeTyping
        property point position
        property var extraArguments: []
        command: ["python3", Quickshell.env("BINGUX_TEST_NATIVE_INPUT"), position.x.toString(), position.y.toString()].concat(extraArguments)

    }
    Component { id: externalSettingsComponent; BinguxSettings {} }
    Component { id: samplePreviewComponent; WidgetPreview { width: 240; height: 160 } }
    Connections {
        target: desktopCustomiser
        function onDraggedIdChanged() { console.log('DRAG_STATE', desktopCustomiser.draggedId); }
        function onHoverZoneChanged() { console.log('DROP_STATE', desktopCustomiser.hoverZone); }
    }
    Connections {
        target: terminalSidebar.contentSelector
        function onVisibleChanged() { console.log('SELECTOR_VISIBLE', terminalSidebar.contentSelector.visible); }
    }
    Connections {
        target: layoutTest.findChild(terminalSidebar.contentItem, 'sidebarContentPicker')
        function onClicked() { console.log('SELECTOR_CLICKED'); }
        function onPressedChanged() { console.log('SELECTOR_PRESSED', target.pressed); }
    }
    TestCase {
        id: layoutTest
        // Start on a mapped surface; the editor window is created by this test.
        parent: topBar.contentItem
        when: topBar.visible && BinguxPreferences.loaded && ControlCentreServices.preferencesReady && dock.appGroupsInitialised
        function clickNative(item, window, x, y, right) {
            nativeTyping.position = DesktopEditing.point(item, window, x, y);
            nativeTyping.extraArguments = [right ? '--right-click' : '--click-only'];
            nativeTyping.running = true;
            tryCompare(nativeTyping, 'running', false, 3000, 'The native click releases its button');
        }
        function dragNative(item, window, x, y, destination) {
            const editor = binguxSettings.customiser;
            const before = JSON.stringify([editor.desktop, editor.layout]);
            nativeTyping.position = DesktopEditing.point(item, window, x, y);
            nativeTyping.extraArguments = ['--drag-to', destination.x.toString(), destination.y.toString()];
            nativeTyping.running = true;
            tryVerify(() => JSON.stringify([editor.desktop, editor.layout]) !== before, 4000, 'The native drop changes the draft layout');
            tryVerify(() => binguxSettings.customiser.draggedId === '', 4000, 'The compositor completes the widget drag');
            tryCompare(nativeTyping, 'running', false, 3000);
            nativeTyping.extraArguments = [];
        }
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
            verify(DesktopEditing.sources.search && DesktopEditing.sources['control-network'], 'The editor keeps the actual widgets registered for dragging');
            const previewSpecs = DesktopLayout.layoutWidgets.concat(DesktopLayout.decorationWidgets, DesktopLayout.widgets, DesktopLayout.controlWidgets);
            for (const spec of previewSpecs) {
                const sample = samplePreviewComponent.createObject(editor.contentItem, {widgetId: spec.id});
                tryVerify(() => sample.previewControl !== null, 1500, 'Sample preview renders: ' + spec.id);
                verify(sample.visualItem.width > 0 && sample.visualItem.height > 0, 'Sample preview has geometry: ' + spec.id);
                wait(30);
                sample.destroy();
            }
            wait(100);
            const originalPanel = terminalSidebar.contentType;
            const picker = findChild(terminalSidebar.contentItem, 'sidebarContentPicker');
            clickNative(picker, terminalSidebar.editWindow, 20, picker.height / 2, false);
            tryCompare(terminalSidebar.contentSelector, 'visible', true, 3000);
            const notesChoice = findChild(terminalSidebar.contentSelector.contentItem, 'sidebar-select-notes');
            verify(notesChoice);
            tryCompare(notesChoice, 'visible', true);
            tryCompare(terminalSidebar.contentSelector, 'revealScale', 1, 3000);
            clickNative(notesChoice, terminalSidebar.contentSelector.nativeWindow, 20, notesChoice.height / 2, false);
            tryCompare(terminalSidebar, 'contentType', 'notes', 3000, 'The real sidebar picker works while customising');
            tryCompare(terminalSidebar.contentSelector, 'retained', false, 2000, 'The panel selector releases its native surface before dragging');
            compare(DesktopEditing.sources.notes, terminalSidebar.activePanel, 'The palette captures the actual panel, not its menu entry');
            const sidebarEdit = DesktopEditing.surfaces.find(area => area.zoneName === 'sidebar');
            const notesStart = terminalSidebar.activePanel.mapToItem(sidebarEdit, 24, 24);
            clickNative(sidebarEdit, terminalSidebar.editWindow, notesStart.x, notesStart.y, true);
            tryCompare(editor, 'selectedWidget', 'notes', 3000, 'The actual sidebar panel exposes widget settings');
            editor.optionsPage = '';
            wait(200);
            const paletteRect = editor.preview.paletteRect;
            dragNative(terminalSidebar.activePanel, terminalSidebar.editWindow, 24, 24, Qt.point(paletteRect.x + 20, paletteRect.y + 20));
            verify(!editor.layout.sidebar.includes('notes'), 'Dragging the actual sidebar panel to the palette removes it');
            verify(BinguxPreferences.data.desktop.layout.sidebar.includes('notes'), 'Panel removal remains a draft');
            editor.cancel();
            compare(terminalSidebar.contentType, originalPanel, 'Cancel restores the selected sidebar panel');
            editor.open();
            wait(300);
            // Reopening keeps the same QQuickWindow even after preview controls exist.
            const editWindow = editor.nativeWindow.contentItem.Window.window;
            for (let cycle = 0; cycle < 3; cycle++) {
                editor.optionsPage = 'Widget'; editor.selectedWidget = 'search';
                wait(80); editor.cancel(); wait(80); editor.open(); wait(100);
                compare(editor.nativeWindow.contentItem.Window.window, editWindow);
            }
            editor.optionsPage = '';
            const leftEdit = DesktopEditing.surfaces.find(area => area.zoneName === 'top-left');
            const flexTile = findChild(editor.contentItem, 'customise-widget-spring');
            const fixedTile = findChild(editor.contentItem, 'customise-widget-spacer');
            for (let n = 0; n < 2; n++) {
                const rect = leftEdit.screenRect;
                dragNative(flexTile, editor.nativeWindow, flexTile.width / 2, 50, Qt.point(rect.x + Math.min(80, rect.width / 2), rect.y + 16));
            }
            tryVerify(() => topBar.spacingWidgets.length === 2);
            wait(100);
            verify(topBar.spacingWidgets.every(item => item.width > 40), 'Springs expand into available bar space');
            verify(Math.abs(topBar.spacingWidgets[0].width - topBar.spacingWidgets[1].width) <= 1, 'Springs share available space');
            editor.undo(); wait(100);
            compare(topBar.spacingWidgets.length, 1, 'Undo reverses one complete native drop');
            editor.redo(); wait(100);
            compare(topBar.spacingWidgets.length, 2, 'Redo restores the spacing instance');
            const leftRect = leftEdit.screenRect;
            dragNative(fixedTile, editor.nativeWindow, fixedTile.width / 2, 50, Qt.point(leftRect.x + 8, leftRect.y + 16));
            tryVerify(() => topBar.spacingWidgets.length === 3);
            editor.selectedWidget = editor.layout['top-left'].find(id => id.startsWith('spacer:'));
            editor.widgetOption('width', 48);
            wait(100);
            compare(topBar.spacingWidgets.find(item => !item.flexible).width, 48, 'Fixed space uses the saved width');
            editor.cancel(); editor.open(); wait(200);
            compare(topBar.spacingWidgets.length, 0, 'Cancel removes draft spacing instances');
            const labelTile = findChild(editor.contentItem, 'customise-widget-label');
            const labelZone = leftEdit.screenRect;
            editor.selectedWidget = 'label'; editor.widgetOption('label', 'Team');
            dragNative(labelTile, editor.nativeWindow, labelTile.width / 2, 50, Qt.point(labelZone.x + 8, labelZone.y + 16));
            tryVerify(() => topBar.decorationWidgets.length === 1);
            const labelWidget = topBar.decorationWidgets[0];
            compare(labelWidget.parent, leftControls);
            compare(labelWidget.presentation.label, 'Team', 'A new label copies the palette appearance');
            editor.selectedWidget = labelWidget.widgetId;
            editor.widgetOption('label', 'Work');
            tryCompare(labelWidget.presentation, 'label', 'Work');
            verify(labelWidget.width > 0 && labelWidget.presentation.showText);
            editor.put(labelWidget.widgetId, 'dock', 0);
            tryCompare(labelWidget, 'parent', dock.widgetHost);
            editor.selectedContainer = 'dock'; editor.containerDisplay('icons');
            verify(labelWidget.presentation.showIcon && !labelWidget.presentation.showText);
            editor.widgetOption('display', 'both');
            verify(labelWidget.presentation.showIcon && labelWidget.presentation.showText);
            const removedLabel = labelWidget.widgetId;
            editor.put(removedLabel, 'palette', 0);
            verify(!editor.desktop.widgetOptions[removedLabel], 'Removal clears instance appearance');
            editor.undo(); wait(100);
            compare(topBar.decorationWidgets[0].presentation.label, 'Work', 'Undo restores the label and its appearance');
            editor.cancel(); editor.open(); wait(200);
            compare(topBar.decorationWidgets.length, 0, 'Cancel removes draft labels');
            editor.optionsPage = 'Widget'; editor.selectedWidget = 'search';
            wait(100);
            const inspector = findChild(editor.optionsContentItem, 'customiseInspector');
            verify(inspector.y >= topBar.margins.top + topBar.height, 'Top bar options appear below the selected item');

            const label = findChild(editor.optionsContentItem, 'customiseWidgetLabelInput');
            verify(label);
            nativeTyping.position = DesktopEditing.point(label, editor.optionsWindow, 20, 12);
            nativeTyping.running = true;
            tryCompare(label, "text", "Find", 4000);
            tryCompare(nativeTyping, 'running', false, 3000, 'Typing releases the last native key');
            mouseClick(findChild(editor.optionsContentItem, 'customiseIconPickerToggle'), 20, 15);
            const star = findChild(editor.optionsContentItem, 'customise-icon-starred-symbolic');
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
            dragNative(searchTile, editor.nativeWindow, 20, 20, Qt.point(destination.x + 12, destination.y + 12));
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
            const dockPoint = dockEdit.screenRect;
            verify(controlCentre.preferredY + controlCentre.popupHeight <= controlCentre.dockSafeBottom + 1, "The open control centre leaves the dock drop area clear");
            dragNative(appTile, editor.nativeWindow, 20, 20, Qt.point(dockPoint.x + 12, dockPoint.y + 12));
            compare(editor.desktop.dockApps.pinnedApps[0], appId, 'Dragging an app pins it in the live dock preview');
            editor.tab = 'Widgets';
            const ccEdit = DesktopEditing.surfaces.find(area => area.zoneName === 'control-centre');
            const network = ccEdit.entries.find(entry => entry.id === 'control-network').item;
            const bluetooth = ccEdit.entries.find(entry => entry.id === 'control-bluetooth').item;
            dragNative(network, controlCentre.nativeWindow, 20, 20, DesktopEditing.point(bluetooth, controlCentre.nativeWindow, bluetooth.width - 2, bluetooth.height / 2));
            compare(editor.desktop.controlOrder[0], 'bluetooth', 'Dragging the actual network tile reorders the control centre');
            const reorderedControls = editor.desktop.controlOrder.slice();
            const originalControls = editor.desktop.controlCentre;
            editor.change('controlCentre', Object.assign({}, originalControls, {nightLight: false, power: false}));
            editor.change('controlOrder', ['nightLight', 'network', 'power', 'bluetooth', 'vpn', 'dnd', 'awake']);
            wait(200);
            verify(!controlCentre.controlVisible('nightLight') && !controlCentre.controlVisible('power'));
            dragNative(network, controlCentre.nativeWindow, 20, 20, DesktopEditing.point(bluetooth, controlCentre.nativeWindow, bluetooth.width - 2, bluetooth.height / 2));
            compare(JSON.stringify(editor.desktop.controlOrder.slice(0, 4)), JSON.stringify(['nightLight', 'power', 'bluetooth', 'network']), 'Hidden tiles do not shift the visible drop position');
            wait(100);
            compare(controlCentre.controlCell('bluetooth').column, 0);
            compare(controlCentre.controlCell('network').column, 1);
            editor.change('controlOrder', reorderedControls);
            editor.change('controlCentre', originalControls);
            wait(300);
            const originalNetworkParent = network.parent;
            const controlsDock = dockEdit.screenRect;
            dragNative(network, controlCentre.nativeWindow, 20, 20, Qt.point(controlsDock.x + 12, controlsDock.y + 12));
            compare(network.parent, dock.widgetHost, 'The actual network widget moves into the dock');
            verify(network.barLayout, 'A dock control uses its compact presentation');
            compare(network.implicitHeight, Theme.barHeight);
            tryCompare(network, 'height', Theme.barHeight, 2000, 'The moved tile finishes its compact layout before the next drag');
            wait(100);
            verify(!editor.desktop.controlOrder.includes('network'), 'Moving a control removes its old placement');
            dragNative(network, dock, network.width / 2, network.height / 2, DesktopEditing.point(bluetooth, controlCentre.nativeWindow, 2, 20));
            compare(network.parent, originalNetworkParent, 'Moving back restores the original control-centre widget');
            verify(!network.barLayout);
            compare(network.implicitHeight, 104);
            const done = findChild(editor.contentItem, 'customiseApply');
            const donePoint = done.mapToItem(editor.preview, done.width, done.height);
            verify(donePoint.x <= editor.width - editor.rightInset, 'Done remains outside the sidebar');
            editor.put('search', 'top-right', 0);
            editor.put('clock', 'dock', 0);
            editor.put('controls', 'dock', 1);
            editor.put('metrics', 'top-left', 0);
            editor.put('control-network', 'top-left', 1);
            editor.put('notes', 'sidebar', 0);
            editor.put('control-bluetooth', 'control-centre', 0);
            editor.selectedContainer = 'dock'; editor.containerDisplay('text');
            editor.selectedWidget = 'clock'; editor.widgetOption('display', 'both'); editor.widgetOption('label', 'Clock'); editor.widgetOption('icon', 'starred-symbolic');
            editor.change('sidebarEdge', 'left');
            editor.change('dockSize', 40);
            editor.change('dockAlignment', 'left');
            editor.put('spring', 'top-left', 1);
            editor.put('spacer', 'top-right', 0);
            const savedSpace = editor.layout['top-right'].find(id => id.startsWith('spacer:'));
            editor.selectedWidget = savedSpace; editor.widgetOption('width', 36);
            editor.put('icon', 'dock', 2);
            const savedIcon = editor.layout.dock.find(id => id.startsWith('icon:'));
            editor.selectedWidget = savedIcon;
            editor.widgetOption('icon', 'user-home-symbolic');
            editor.widgetOption('label', 'Home');
            editor.widgetOption('display', 'icons');
            mouseClick(done, done.width / 2, done.height / 2);
            tryCompare(binguxSettings, 'busy', false, 4000);
            tryCompare(editor, 'visible', false, 2000);
            tryCompare(clockPill, 'parent', dock.widgetHost);
            compare(systemPill.parent, dock.widgetHost);
            const iconWidget = topBar.decorationWidgets.find(item => item.widgetId === savedIcon);
            compare(iconWidget.parent, dock.widgetHost);
            compare(iconWidget.presentation.icon, 'user-home-symbolic');
            verify(BinguxPreferences.data.desktop.layout.dock.includes(savedIcon));
            mouseClick(iconWidget, iconWidget.width / 2, iconWidget.height / 2, Qt.RightButton, Qt.ShiftModifier);
            tryCompare(widgetMenu, 'visible', true);
            compare(widgetMenu.widgetId, savedIcon);
            widgetMenu.visible = false;
            compare(systemPill.presentation.showIcon, false);
            compare(systemPill.presentation.showText, true);
            compare(clockPill.presentation.label, 'Clock');
            compare(clockPill.presentation.icon, 'starred-symbolic');
            compare(clockPill.presentation.showIcon, true);
            tryVerify(() => clockPill.x < systemPill.x, 2000, 'Dock widgets keep the configured order');
            compare(metricsPill.parent, leftControls);
            compare(network.parent, leftControls, 'The saved control uses its new top-bar container');
            verify(BinguxPreferences.data.desktop.layout['top-right'].includes(savedSpace), 'The saved layout retains its spacing instance');
            compare(BinguxPreferences.data.desktop.widgetOptions[savedSpace].width, 36);
            compare(topBar.spacingWidgets.find(item => item.widgetId === savedSpace).width, 36, 'The actual bar uses its saved spacing after editor close');
            verify(ControlCentreServices.active, 'Moved controls keep their service state current while the popup is closed');
            compare(searchPill.parent, rightControls);
            compare(dock.iconSize, 40);
            compare(terminalSidebar.edge, 'left', 'Saved sidebar edge remains applied after leaving the editor');
            compare(terminalSidebar.contentTypes[0].id, 'notes');
            tryVerify(() => dock.widgetHost.implicitWidth > 0, 2000);
            tryCompare(widgetMenu, 'retained', false);
            tryCompare(topBar.margins, 'top', 0);
            verify(waitForRendering(network), 'The saved bar placement has reached the native window');
            clickNative(network, topBar, network.width / 2, network.height / 2, false);
            tryCompare(controlCentre, 'visible', true, 3000);
            verify(controlCentre.detailOpen, 'The moved network control still opens its real settings');
            compare(controlCentre.detailPage, 'network');
            compare(controlCentre.anchorItem, network);
            compare(controlCentre.anchorWindow, topBar);
            controlCentre.visible = false;
            wait(200);
            controlCentre.visible = true;
            wait(400);
            compare(controlCentre.controlCell('bluetooth').column, 0, 'The live control centre follows the saved tile order');
            compare(controlCentre.controlCell('network').column, -1, 'A moved control is not duplicated in the control centre');
            const networkWidget = DesktopEditing.sources['control-network'];
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
            terminalSidebar.selectContent('notes'); terminalSidebar.open(); wait(250);
            terminalSidebar.popOut();
            tryCompare(terminalSidebar.detachedSurface, 'visible', true);
            const sidebarPanel = terminalSidebar.activePanel;
            const floatingWindow = terminalSidebar.detachedSurface.contentItem.Window.window;
            editor.open(); wait(300);
            verify(terminalSidebar.detached, 'Editing does not change the saved popped-out state');
            verify(!terminalSidebar.detachedSurface.visible, 'Only the edge sidebar is shown during editing');
            verify(terminalSidebar.editWindow.visible);
            compare(terminalSidebar.activePanel, sidebarPanel, 'The editor reuses the actual sidebar content');
            for (const edge of ['left', 'top', 'right']) {
                editor.change('sidebarEdge', edge); wait(350);
                const area = DesktopEditing.surfaces.find(surface => surface.zoneName === 'sidebar');
                const rect = area.screenRect;
                verify(rect.width > 0 && rect.height > 0, 'The sidebar has real editor geometry at ' + edge);
                compare(Math.round(rect.x), edge === 'right' ? editor.width - Math.round(rect.width) : 0);
                compare(Math.round(rect.y), edge === 'top' ? Theme.barHeight : 0);
                compare(topBar.margins.top, 0, 'The top bar keeps its desktop alignment beside the sidebar');
                if (edge === 'top') compare(topBar.margins.top + topBar.height, rect.y, 'The top sidebar starts below the bar without overlap');
                verify(rect.x + rect.width <= editor.width && rect.y + rect.height <= editor.height);
            }
            editor.cancel(); wait(350);
            verify(terminalSidebar.detachedSurface.visible);
            compare(terminalSidebar.detachedSurface.contentItem.Window.window, floatingWindow, 'Leaving restores the same floating window');
            compare(terminalSidebar.activePanel, sidebarPanel, 'Leaving preserves the original sidebar content');
            editor.open(); wait(250);
            const savedEdge = terminalSidebar.edge === 'left' ? 'right' : 'left';
            editor.change('sidebarEdge', savedEdge);
            editor.apply();
            tryCompare(editor, 'visible', false, 4000);
            verify(terminalSidebar.detached, 'Saving an edge does not dock a popped-out sidebar');
            verify(terminalSidebar.detachedSurface.visible);
            compare(terminalSidebar.edge, savedEdge);
            compare(terminalSidebar.activePanel, sidebarPanel);
            terminalSidebar.dockBack(); wait(200);
            const crowded = DesktopLayout.defaults();
            crowded['top-left'] = crowded['top-left'].concat(crowded['top-center'], crowded['top-right']);
            crowded['top-center'] = []; crowded['top-right'] = [];
            BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {desktop: Object.assign({}, BinguxPreferences.data.desktop, {layout: crowded})});
            topBar.margins.right = topBar.screen.width - 300;
            tryVerify(() => topBar.overflowItems.length > 0, 2000);
            tryVerify(() => overflowButton.visible, 2000);
            verify(topBar.overflowItems.every(item => item.parent === overflowColumn));
            clickNative(overflowButton, topBar.windowFor(overflowButton), overflowButton.width / 2, overflowButton.height / 2, false);
            tryCompare(barOverflow, 'visible', true, 3000);
            barOverflow.visible = false;
            console.log('DESKTOP_LAYOUT_LIVE_PASS');
            layoutReport.setText('PASS');
            } catch (error) { console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack); layoutReport.setText("FAIL " + error.stack); }
        }
    }
