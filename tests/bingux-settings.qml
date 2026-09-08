import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_SETTINGS_REPORT") }
    BinguxSettings { id: settings; visible: true }
    TestCase {
        id: test
        parent: settings.contentItem
        when: settings.visible
        property bool captured: false
        function screenshot(page) {
            settings.page = page;
            wait(100);
            captured = false;
            settings.contentItem.grabToImage(result => { captured = result.saveToFile('/tmp/bingux-settings-' + page.toLowerCase() + (settings.wideLayout ? '' : '-narrow') + '.png'); });
            tryCompare(test, 'captured', true, 2000);
        }
        function test_settings() {
            try {
            tryCompare(settings, 'ready', true, 3000);
            tryCompare(settings, 'busy', false, 3000);
            compare(settings.draft.previews.maxMegabytes, 20);
            for (const page of ['Search', 'AI', 'Previews', 'Desktop']) screenshot(page);
            settings.page = 'Search';
            const row = findChild(settings.contentItem, 'settingsApplications');
            const toggle = findChild(settings.contentItem, 'settingsApplicationsSwitch');
            verify(toggle.checked);
            mouseClick(row, 80, row.height / 2);
            verify(toggle.checked, 'Opening provider details leaves enabled state unchanged');
            compare(settings.pageTitle, 'Applications');
            mouseClick(findChild(settings.contentItem, 'settingsNavigationToggle'));
            compare(settings.searchPage.detail, '');
            settings.searchPage.filter = 'does-not-exist';
            compare(settings.searchPage.matchingProviders, 0);
            compare(settings.searchPage.matchingEngines, 0);
            settings.searchPage.filter = '';
            settings.page = 'AI'; settings.page = 'Search';
            settings.setProvider('applications', false);
            verify(settings.draft.search.disabledProviders.includes('applications'));
            settings.setProvider('applications', true);
            const providers = settings.searchPage;
            providers.detail = "";
            providers.editEngine(null);
            providers.engineName = "Documentation";
            providers.engineShortcut = "docs";
            providers.engineUrl = "https://docs.example.test/search?q={query}";
            providers.storeEngine();
            verify(!providers.editing);
            compare(settings.draft.search.engines.length, 2);
            const engine = settings.draft.search.engines[1];
            settings.update("search", "defaultEngine", engine.id);
            providers.toggleEngine(engine);
            verify(settings.draft.search.engines[1].enabled, "Default engine cannot be disabled");
            providers.editEngine(engine);
            providers.engineUrl = "javascript:{query}";
            providers.storeEngine();
            verify(providers.editing && providers.error.length > 0, "Invalid URLs stay in the form");
            providers.editing = false;
            settings.save();
            tryCompare(settings, "busy", false, 4000);
            settings.read(); tryCompare(settings, "busy", false, 4000);
            compare(settings.draft.search.defaultEngine, engine.id);
            compare(settings.draft.search.engines[1].url, "https://docs.example.test/search?q={query}");
            settings.page = 'AI';
            const advanced = findChild(settings.contentItem, 'settingsAdvanced');
            mouseClick(advanced);
            verify(settings.advancedOpen);
            verify(findChild(settings.contentItem, 'settingsExecutable').visible);
            wait(200);
            compare(advanced.navigationRotation, 90);
            settings.setProvider('conversions', false);
            settings.update('previews', 'maxMegabytes', 5);
            settings.update('desktop', 'metrics', false);
            settings.save();
            wait(150);
            tryCompare(settings, 'busy', false, 3000);
            report.setText('Save result: ' + settings.status);
            compare(settings.status, 'Saved');
            compare(BinguxPreferences.data.previews.maxMegabytes, 5);
            settings.read();
            wait(150);
            tryCompare(settings, 'busy', false, 3000);
            verify(settings.draft.search.disabledProviders.includes('conversions'));
            compare(settings.draft.desktop.metrics, false);
            settings.update('previews', 'maxMegabytes', 21);
            settings.save();
            wait(150);
            tryCompare(settings, 'busy', false, 3000);
            verify(settings.status.includes('between 1 and 20'));
            settings.read();
            wait(150);
            tryCompare(settings, 'busy', false, 3000);
            compare(settings.draft.previews.maxMegabytes, 5);
            settings.width = 640; settings.height = 480;
            screenshot('Search');
            verify(!findChild(settings.contentItem, 'settingsNavigation').visible, 'Narrow window collapses sidebar');
            mouseClick(findChild(settings.contentItem, 'settingsNavigationToggle'));
            verify(settings.navigationOpen);
            mouseClick(findChild(settings.contentItem, 'settingsNavDesktop'));
            compare(settings.page, 'Desktop');
            verify(!settings.navigationOpen);
            screenshot('Desktop');
            settings.width = 960; settings.height = 700;
            settings.page = 'Search';
            const nav = findChild(settings.contentItem, 'settingsNavSearch');
            nav.forceActiveFocus();
            keyClick(Qt.Key_Down);
            compare(settings.page, 'AI');
            settings.update('previews', 'maxMegabytes', 7);
            settings.visible = false;
            settings.visible = true;
            wait(100);
            verify(settings.dirty, 'Reopening preserves unsaved changes');
            compare(settings.draft.previews.maxMegabytes, 7);
            report.setText('BINGUX_SETTINGS_PASS');
            } catch (error) { report.setText('FAIL: ' + error + '\nStatus: ' + settings.status + '\n' + error.stack); throw error; }
        }
    }
}
