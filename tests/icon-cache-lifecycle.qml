import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    PanelWindow {
        id: host
        implicitWidth: 100
        implicitHeight: 100
    }
    Component {
        id: icon
        OsIconImage {
            width: 32
            height: 32
        }
    }
    TestCase {
        parent: host.contentItem
        when: host.visible
        function initTestCase() {
            report.setText("RUNNING");
        }
        function cleanupTestCase() {
            report.setText("FAILURES " + qtest_results.failCount);
        }
        function test_shared_icon_lifetime() {
            const source = Quickshell.iconPath("window-close-symbolic").toString();
            const first = icon.createObject(host.contentItem, {
                source
            });
            const second = icon.createObject(host.contentItem, {
                source
            });
            tryVerify(() => OsIcons.cache.references.get(source) === 2);
            tryVerify(() => !!OsIcons.sources[source], 5000);
            const originalLimit = OsIcons.cache.maxEntries;
            OsIcons.cache.maxEntries = 0;
            first.destroy();
            tryVerify(() => OsIcons.cache.references.get(source) === 1);
            verify(!!OsIcons.sources[source], "The shared icon stays resident");
            second.destroy();
            tryVerify(() => !OsIcons.cache.references.has(source));
            tryVerify(() => !OsIcons.sources[source]);
            verify(!OsIcons.requested[source], "A future user can request the evicted icon again");
            OsIcons.cache.maxEntries = originalLimit;
            const third = icon.createObject(host.contentItem, {
                source
            });
            tryVerify(() => !!OsIcons.sources[source], 5000);
            third.destroy();
        }
    }
}
