import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CUSTOMISE_REPORT") }
    BinguxSettings { id: settings; visible: true }
    TestCase {
        id: test
        when: settings.visible
        property bool captured: false
        function capture(name) {
            captured = false;
            settings.customiser.preview.grabToImage(result => { captured = result.saveToFile('/tmp/bingux-customise-' + name + '.png'); });
            tryCompare(test, 'captured', true, 3000);
        }
        function test_editor() {
            try {
            tryCompare(settings, 'ready', true, 4000);
            tryCompare(settings, 'busy', false, 4000);
            settings.customiser.open();
            const editor = settings.customiser;
            tryVerify(() => editor.wallpaper.length > 0, 15000);
            wait(600);
            verify(editor.visible);
            const before = JSON.stringify(settings.draft);
            capture('widgets');
            const chip = findChild(editor.contentItem, 'customise-widget-search');
            const target = findChild(editor.contentItem, 'customise-zone-dock');
            verify(chip && target);
            const end = target.mapToItem(chip, target.width / 2, target.height / 2);
            mousePress(chip, 20, 18);
            mouseMove(chip, 35, 20, 50);
            mouseMove(chip, end.x, end.y, 150);
            mouseRelease(chip, end.x, end.y, Qt.LeftButton, Qt.NoModifier, 50);
            compare(editor.layout.dock[0], 'search', 'Actual drag moves the widget');
            compare(editor.layout['top-left'].length, 0);
            compare(JSON.stringify(settings.draft), before, 'Preview does not change saved settings');
            editor.cancel();
            compare(JSON.stringify(settings.draft), before);
            editor.open();
            compare(editor.layout['top-left'][0], 'search', 'Cancel discards the move');
            editor.put('search', 'dock', 0);
            wait(200);
            const palette = findChild(editor.contentItem, 'customisePalette');
            const notes = findChild(palette, 'customise-widget-notes');
            const sidebar = findChild(editor.contentItem, 'customise-zone-sidebar');
            const notesEnd = sidebar.mapToItem(notes, 24, 10);
            mousePress(notes, 20, 18);
            mouseMove(notes, 36, 20, 50);
            mouseMove(notes, notesEnd.x, notesEnd.y, 150);
            mouseRelease(notes, notesEnd.x, notesEnd.y, Qt.LeftButton, Qt.NoModifier, 50);
            compare(editor.layout.sidebar[0], 'notes', 'Dragging from the centre palette moves a sidebar widget');
            editor.tab = 'Apps';
            tryVerify(() => editor.applications.length > 0, 3000);
            const appId = editor.appId(editor.applications[0].id);
            wait(100);
            const appChip = findChild(palette, 'customise-widget-app:' + appId);
            const appDock = findChild(editor.contentItem, 'customise-zone-dock-apps');
            verify(appChip && appDock);
            const appEnd = appDock.mapToItem(appChip, 24, 24);
            mousePress(appChip, 20, 18);
            mouseMove(appChip, 36, 20, 50);
            mouseMove(appChip, appEnd.x, appEnd.y, 150);
            mouseRelease(appChip, appEnd.x, appEnd.y, Qt.LeftButton, Qt.NoModifier, 50);
            compare(editor.desktop.dockApps.pinnedApps[0], appId, 'An installed app can be dragged into the dock');
            capture('apps');
            editor.tab = 'Widgets';
            const network = findChild(editor.contentItem, 'customise-control-control-network');
            const controls = findChild(editor.contentItem, 'customise-zone-control-centre');
            verify(network && controls);
            const controlsEnd = controls.mapToItem(network, controls.width - 4, controls.height - 4);
            mousePress(network, 20, 18);
            mouseMove(network, 36, 20, 50);
            mouseMove(network, controlsEnd.x, controlsEnd.y, 150);
            mouseRelease(network, controlsEnd.x, controlsEnd.y, Qt.LeftButton, Qt.NoModifier, 50);
            compare(editor.desktop.controlOrder[0], 'bluetooth', 'Control-centre tiles can be reordered by dragging');
            editor.change('dockSize', 40);
            editor.change('dockAlignment', 'left');
            editor.optionsPage = 'Dock'; wait(200); capture('dock');
            editor.apply();
            tryCompare(settings, 'busy', false, 4000);
            tryCompare(editor, 'visible', false, 2000);
            settings.read(); tryCompare(settings, 'busy', false, 4000);
            compare(settings.draft.desktop.layout.dock[0], 'search');
            compare(settings.draft.desktop.dockApps.pinnedApps[0], appId, 'App pins survive save and reload');
            compare(settings.draft.desktop.controlOrder[0], 'bluetooth', 'Control-centre order survives save and reload');
            compare(settings.draft.desktop.layout.sidebar[0], 'notes');
            compare(settings.draft.desktop.dockSize, 40);
            compare(settings.draft.desktop.dockAlignment, 'left');
            console.log('DESKTOP_CUSTOMISE_PASS');
            report.setText('PASS');
            } catch (error) { console.error("CUSTOMISE_TEST_FAILED", error.message, error.stack); report.setText('FAIL ' + error.stack); }
        }
    }
}
